import {
  BadRequestException,
  Body,
  Controller,
  Delete,
  ForbiddenException,
  Get,
  HttpCode,
  HttpException,
  HttpStatus,
  NotFoundException,
  Param,
  Post,
  Req,
  UnauthorizedException,
  UseGuards,
} from "@nestjs/common";
import type { Request } from "express";
import { AuthGuard } from "../auth/auth.guard.js";
import type { UserContext } from "../auth/user-context.interface.js";
import { SyncService } from "../sync/sync.service.js";
import { LocksService } from "./locks.service.js";
import { inviteLink, workspaceName } from "./server-config.js";
import { WorkspaceService } from "./workspace.service.js";
import { MAX_NAME_LENGTH } from "./workspace.store.js";

function userOf(req: Request): UserContext {
  return (req as unknown as Record<string, unknown>).user as UserContext;
}

function requireOwner(ctx: UserContext): void {
  if (ctx.role !== "owner") {
    throw new ForbiddenException("Only the owner of the server can do this");
  }
}

/** How many join attempts one address may make per window. */
const JOIN_ATTEMPTS = 10;
const JOIN_WINDOW_MS = 60_000;

/** Joining by invite: the one call that needs no token. */
@Controller("v1")
export class JoinController {
  private readonly attempts = new Map<string, number[]>();

  constructor(private readonly workspace: WorkspaceService) {}

  private throttle(address: string): void {
    const now = Date.now();
    const recent = (this.attempts.get(address) ?? []).filter(
      (at) => now - at < JOIN_WINDOW_MS,
    );
    if (recent.length >= JOIN_ATTEMPTS) {
      throw new HttpException(
        "Too many attempts",
        HttpStatus.TOO_MANY_REQUESTS,
      );
    }
    recent.push(now);
    this.attempts.set(address, recent);
    if (this.attempts.size > 10_000) this.attempts.clear();
  }

  @Post("join")
  @HttpCode(200)
  join(@Body() body: { code?: unknown; name?: unknown }, @Req() req: Request) {
    this.throttle(req.ip ?? "unknown");
    const name =
      typeof body?.name === "string"
        ? body.name.trim().slice(0, MAX_NAME_LENGTH)
        : "";
    if (!name) throw new BadRequestException("name is required");
    const code = typeof body?.code === "string" ? body.code.trim() : "";
    const joined = code ? this.workspace.store.join(code, name) : null;
    if (!joined) {
      throw new UnauthorizedException({ error: "invite_invalid" });
    }
    const { member, token } = joined;
    return {
      token,
      member: { id: member.id, name: member.name, role: member.role },
      workspace: { name: workspaceName() },
    };
  }
}

/** Who is on this server, the invites that let someone join, the shared
 * profiles, and the API keys scripts use for all of that. */
@Controller("v1")
@UseGuards(AuthGuard)
export class WorkspaceController {
  constructor(
    private readonly workspace: WorkspaceService,
    private readonly locks: LocksService,
    private readonly sync: SyncService,
  ) {}

  @Get("me")
  me(@Req() req: Request) {
    const ctx = userOf(req);
    return {
      member: {
        id: ctx.sub ?? "sync-token",
        name: ctx.name ?? "Owner",
        role: ctx.role ?? "owner",
      },
      workspace: { name: workspaceName() },
    };
  }

  @Get("members")
  members() {
    return { members: this.workspace.store.listMembers() };
  }

  @Delete("members/:id")
  @HttpCode(204)
  removeMember(@Param("id") id: string, @Req() req: Request): void {
    requireOwner(userOf(req));
    const result = this.workspace.store.removeMember(id);
    if (result === "not_found") throw new NotFoundException();
    if (result === "owner") {
      throw new BadRequestException("The owner cannot be removed");
    }
    this.locks.releaseAllOf(id);
  }

  @Post("invites")
  @HttpCode(200)
  createInvite(@Req() req: Request) {
    const ctx = userOf(req);
    requireOwner(ctx);
    const invite = this.workspace.store.createInvite("member", ctx.sub ?? null);
    return {
      id: invite.id,
      code: invite.code,
      link: inviteLink(invite.code),
      createdAt: invite.createdAt,
      expiresAt: invite.expiresAt,
    };
  }

  @Get("invites")
  invites(@Req() req: Request) {
    requireOwner(userOf(req));
    return {
      invites: this.workspace.store
        .listInvites()
        .map(({ id, createdAt, expiresAt }) => ({ id, createdAt, expiresAt })),
    };
  }

  /** The profiles shared here, who shared them, and who has each one open. */
  @Get("profiles")
  async profiles() {
    const openBy = new Map(
      this.locks.list().map((lock) => [lock.profileId, lock.lockedByEmail]),
    );
    const shared = await this.sync.sharedProfiles();
    return {
      profiles: shared.map(({ id, metadata }) => {
        const m = metadata ?? {};
        const by = m.shared_by as { id?: unknown; name?: unknown } | undefined;
        return {
          id,
          name: typeof m.name === "string" ? m.name : null,
          browser: typeof m.browser === "string" ? m.browser : null,
          tags: Array.isArray(m.tags)
            ? m.tags.filter((tag) => typeof tag === "string")
            : [],
          sharedBy:
            by && typeof by.id === "string" && typeof by.name === "string"
              ? { id: by.id, name: by.name }
              : null,
          updatedAt: typeof m.updated_at === "number" ? m.updated_at : null,
          encrypted: metadata === null,
          openBy: openBy.get(id) ?? null,
        };
      }),
    };
  }

  @Get("api-keys")
  apiKeys(@Req() req: Request) {
    requireOwner(userOf(req));
    return { keys: this.workspace.store.listApiKeys() };
  }

  @Post("api-keys")
  @HttpCode(200)
  createApiKey(@Body() body: { name?: unknown }, @Req() req: Request) {
    const ctx = userOf(req);
    requireOwner(ctx);
    const name =
      typeof body?.name === "string"
        ? body.name.trim().slice(0, MAX_NAME_LENGTH)
        : "";
    if (!name) throw new BadRequestException("name is required");
    return this.workspace.store.createApiKey(name, ctx.sub ?? null);
  }

  @Delete("api-keys/:id")
  @HttpCode(204)
  revokeApiKey(@Param("id") id: string, @Req() req: Request): void {
    requireOwner(userOf(req));
    if (!this.workspace.store.revokeApiKey(id)) throw new NotFoundException();
  }

  @Delete("invites/:id")
  @HttpCode(204)
  revokeInvite(@Param("id") id: string, @Req() req: Request): void {
    requireOwner(userOf(req));
    if (!this.workspace.store.revokeInvite(id)) throw new NotFoundException();
  }
}

/** The profile locks, in the protocol the app also speaks with Donut's cloud. */
@Controller("v1/profile-locks")
@UseGuards(AuthGuard)
export class LocksController {
  constructor(private readonly locks: LocksService) {}

  private holder(req: Request): { id: string; name: string } {
    const ctx = userOf(req);
    return { id: ctx.sub ?? "sync-token", name: ctx.name ?? "Owner" };
  }

  @Get()
  list() {
    return this.locks.list();
  }

  @Post(":id")
  @HttpCode(200)
  acquire(@Param("id") profileId: string, @Req() req: Request) {
    const { id, name } = this.holder(req);
    return this.locks.acquire(profileId, id, name);
  }

  @Post(":id/heartbeat")
  @HttpCode(200)
  heartbeat(@Param("id") profileId: string, @Req() req: Request) {
    return { refreshed: this.locks.heartbeat(profileId, this.holder(req).id) };
  }

  @Delete(":id")
  @HttpCode(204)
  release(@Param("id") profileId: string, @Req() req: Request): void {
    if (!this.locks.release(profileId, this.holder(req).id)) {
      throw new NotFoundException();
    }
  }
}
