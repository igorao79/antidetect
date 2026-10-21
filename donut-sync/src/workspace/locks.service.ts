import { Injectable } from "@nestjs/common";
import { SyncService } from "../sync/sync.service.js";

export interface ProfileLock {
  profileId: string;
  /** The member holding the lock. */
  lockedBy: string;
  /** The member's name. Named after the Donut cloud field the app reads. */
  lockedByEmail: string;
  lockedAt: string;
  expiresAt: string;
}

/** A lock not renewed for this long is released: its app went away. */
export const LOCK_TTL_MS = 120_000;

/**
 * Which member has which shared profile open. Kept in memory: a lock lives
 * only as long as its app renews it, so a restart loses at most one renewal.
 * Every change is announced on the subscribe stream as a `lock` event.
 */
@Injectable()
export class LocksService {
  private readonly locks = new Map<string, ProfileLock>();

  constructor(private readonly sync: SyncService) {}

  private sweep(now = Date.now()): void {
    for (const [profileId, lock] of this.locks) {
      if (Date.parse(lock.expiresAt) <= now) {
        this.locks.delete(profileId);
        this.announce(profileId);
      }
    }
  }

  private announce(profileId: string): void {
    this.sync.emitChange({ type: "lock", key: `profile-locks/${profileId}` });
  }

  list(): ProfileLock[] {
    this.sweep();
    return [...this.locks.values()];
  }

  /** Take the lock, or renew it when this member already holds it. */
  acquire(
    profileId: string,
    memberId: string,
    memberName: string,
  ):
    | { success: true }
    | { success: false; lockedBy: string; lockedByEmail: string } {
    this.sweep();
    const held = this.locks.get(profileId);
    if (held && held.lockedBy !== memberId) {
      return {
        success: false,
        lockedBy: held.lockedBy,
        lockedByEmail: held.lockedByEmail,
      };
    }
    const now = Date.now();
    this.locks.set(profileId, {
      profileId,
      lockedBy: memberId,
      lockedByEmail: memberName,
      lockedAt: held?.lockedAt ?? new Date(now).toISOString(),
      expiresAt: new Date(now + LOCK_TTL_MS).toISOString(),
    });
    if (!held) this.announce(profileId);
    return { success: true };
  }

  /** Renew a lock this member holds. False when it is gone or someone else's. */
  heartbeat(profileId: string, memberId: string): boolean {
    this.sweep();
    const held = this.locks.get(profileId);
    if (!held || held.lockedBy !== memberId) return false;
    held.expiresAt = new Date(Date.now() + LOCK_TTL_MS).toISOString();
    return true;
  }

  release(profileId: string, memberId: string): boolean {
    this.sweep();
    const held = this.locks.get(profileId);
    if (!held || held.lockedBy !== memberId) return false;
    this.locks.delete(profileId);
    this.announce(profileId);
    return true;
  }

  /** Drop every lock a removed member held. */
  releaseAllOf(memberId: string): void {
    for (const [profileId, lock] of this.locks) {
      if (lock.lockedBy === memberId) {
        this.locks.delete(profileId);
        this.announce(profileId);
      }
    }
  }
}
