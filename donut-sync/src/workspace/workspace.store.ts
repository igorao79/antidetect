import { createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

export type MemberRole = "owner" | "member";

export interface Member {
  id: string;
  name: string;
  role: MemberRole;
  createdAt: string;
}

export interface Invite {
  id: string;
  role: MemberRole;
  createdAt: string;
  expiresAt: string;
}

/** An invite as it is created: the code exists only in this answer. */
export interface CreatedInvite extends Invite {
  code: string;
}

export const INVITE_TTL_MS = 7 * 24 * 60 * 60 * 1000;
export const MAX_NAME_LENGTH = 64;

function hash(secret: string): string {
  return createHash("sha256").update(secret).digest("hex");
}

function secret(): string {
  return randomBytes(24).toString("base64url");
}

/**
 * Who belongs to this server: members with their personal tokens, the
 * invites that let someone join, and which member first shared each profile.
 * Tokens and invite codes are stored only as SHA-256 hashes.
 */
export class WorkspaceStore {
  private readonly db: DatabaseSync;

  constructor(directory: string) {
    mkdirSync(directory, { recursive: true });
    this.db = new DatabaseSync(join(directory, "owl.db"));
    this.db.exec(`
      PRAGMA journal_mode = WAL;
      CREATE TABLE IF NOT EXISTS members (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        role TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL,
        revoked_at TEXT
      );
      CREATE TABLE IF NOT EXISTS invites (
        id TEXT PRIMARY KEY,
        code_hash TEXT NOT NULL UNIQUE,
        role TEXT NOT NULL,
        created_by TEXT,
        created_at TEXT NOT NULL,
        expires_at TEXT NOT NULL,
        used_by TEXT,
        revoked_at TEXT
      );
      CREATE TABLE IF NOT EXISTS profile_owners (
        profile_id TEXT PRIMARY KEY,
        member_id TEXT NOT NULL
      );
    `);
  }

  close(): void {
    this.db.close();
  }

  hasOwner(): boolean {
    const row = this.db
      .prepare(
        "SELECT 1 AS found FROM members WHERE role = 'owner' AND revoked_at IS NULL LIMIT 1",
      )
      .get();
    return Boolean(row);
  }

  /**
   * A fresh invite. An owner invite replaces any earlier unused one, so only
   * the link printed last can claim the server.
   */
  createInvite(role: MemberRole, createdBy: string | null): CreatedInvite {
    const now = new Date();
    if (role === "owner") {
      this.db
        .prepare(
          "UPDATE invites SET revoked_at = ? WHERE role = 'owner' AND used_by IS NULL AND revoked_at IS NULL",
        )
        .run(now.toISOString());
    }
    const code = secret();
    const invite: CreatedInvite = {
      id: randomUUID(),
      role,
      code,
      createdAt: now.toISOString(),
      expiresAt: new Date(now.getTime() + INVITE_TTL_MS).toISOString(),
    };
    this.db
      .prepare(
        "INSERT INTO invites (id, code_hash, role, created_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)",
      )
      .run(
        invite.id,
        hash(code),
        role,
        createdBy,
        invite.createdAt,
        invite.expiresAt,
      );
    return invite;
  }

  /** Invites that can still be used, newest first. Owner invites are not listed. */
  listInvites(): Invite[] {
    return this.db
      .prepare(
        `SELECT id, role, created_at AS createdAt, expires_at AS expiresAt FROM invites
         WHERE used_by IS NULL AND revoked_at IS NULL AND expires_at > ? AND role = 'member'
         ORDER BY created_at DESC`,
      )
      .all(new Date().toISOString()) as unknown as Invite[];
  }

  revokeInvite(id: string): boolean {
    const result = this.db
      .prepare(
        "UPDATE invites SET revoked_at = ? WHERE id = ? AND used_by IS NULL AND revoked_at IS NULL",
      )
      .run(new Date().toISOString(), id);
    return Number(result.changes) > 0;
  }

  /**
   * Turn an invite code into a member and that member's token. Null when the
   * code is unknown, used, revoked or expired, or names an owner the server
   * already has.
   */
  join(code: string, name: string): { member: Member; token: string } | null {
    const now = new Date().toISOString();
    this.db.exec("BEGIN IMMEDIATE");
    try {
      const invite = this.db
        .prepare(
          `SELECT id, role FROM invites WHERE code_hash = ? AND used_by IS NULL
           AND revoked_at IS NULL AND expires_at > ?`,
        )
        .get(hash(code), now) as { id: string; role: MemberRole } | undefined;
      if (!invite || (invite.role === "owner" && this.hasOwner())) {
        this.db.exec("ROLLBACK");
        return null;
      }
      const token = secret();
      const member: Member = {
        id: randomUUID(),
        name,
        role: invite.role,
        createdAt: now,
      };
      this.db
        .prepare(
          "INSERT INTO members (id, name, role, token_hash, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .run(member.id, member.name, member.role, hash(token), now);
      this.db
        .prepare("UPDATE invites SET used_by = ? WHERE id = ?")
        .run(member.id, invite.id);
      this.db.exec("COMMIT");
      return { member, token };
    } catch (error) {
      this.db.exec("ROLLBACK");
      throw error;
    }
  }

  /** The member a token belongs to, unless the member was removed. */
  memberByToken(token: string): Member | null {
    const row = this.db
      .prepare(
        "SELECT id, name, role, created_at AS createdAt FROM members WHERE token_hash = ? AND revoked_at IS NULL",
      )
      .get(hash(token));
    return (row as unknown as Member | undefined) ?? null;
  }

  listMembers(): Member[] {
    return this.db
      .prepare(
        `SELECT id, name, role, created_at AS createdAt FROM members WHERE revoked_at IS NULL
         ORDER BY CASE role WHEN 'owner' THEN 0 ELSE 1 END, created_at`,
      )
      .all() as unknown as Member[];
  }

  /** Remove a member. The owner cannot be removed. */
  removeMember(id: string): "removed" | "not_found" | "owner" {
    const row = this.db
      .prepare("SELECT role FROM members WHERE id = ? AND revoked_at IS NULL")
      .get(id) as { role: MemberRole } | undefined;
    if (!row) return "not_found";
    if (row.role === "owner") return "owner";
    this.db
      .prepare("UPDATE members SET revoked_at = ? WHERE id = ?")
      .run(new Date().toISOString(), id);
    return "removed";
  }

  /** Remember who first put a profile on the server. Later uploads change nothing. */
  recordProfileOwner(profileId: string, memberId: string): void {
    this.db
      .prepare(
        "INSERT OR IGNORE INTO profile_owners (profile_id, member_id) VALUES (?, ?)",
      )
      .run(profileId, memberId);
  }

  profileOwner(profileId: string): string | null {
    const row = this.db
      .prepare("SELECT member_id FROM profile_owners WHERE profile_id = ?")
      .get(profileId) as { member_id: string } | undefined;
    return row?.member_id ?? null;
  }

  forgetProfileOwner(profileId: string): void {
    this.db
      .prepare("DELETE FROM profile_owners WHERE profile_id = ?")
      .run(profileId);
  }
}
