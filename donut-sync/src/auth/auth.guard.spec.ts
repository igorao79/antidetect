import { generateKeyPairSync } from "node:crypto";
import {
  type ExecutionContext,
  Logger,
  UnauthorizedException,
} from "@nestjs/common";
import type { ConfigService } from "@nestjs/config";
import * as jwt from "jsonwebtoken";
import type { WorkspaceService } from "../workspace/workspace.service.js";
import { AuthGuard } from "./auth.guard.js";
import type { UserContext } from "./user-context.interface.js";

const signingKeys = generateKeyPairSync("rsa", {
  modulusLength: 2048,
  publicKeyEncoding: { type: "spki", format: "pem" },
  privateKeyEncoding: { type: "pkcs8", format: "pem" },
});

const otherKeys = generateKeyPairSync("rsa", {
  modulusLength: 2048,
  publicKeyEncoding: { type: "spki", format: "pem" },
  privateKeyEncoding: { type: "pkcs8", format: "pem" },
});

const USER_ID = "11111111-2222-3333-4444-555555555555";
const SELF_HOSTED_TOKEN = "self-hosted-secret";

function guardWith(env: Record<string, string | undefined>): AuthGuard {
  const config = {
    get: (key: string) => env[key],
  } as unknown as ConfigService;
  // No member tokens in these tests: every lookup misses.
  const workspace = {
    store: { memberByToken: () => null, apiKeyByToken: () => null },
  } as unknown as WorkspaceService;
  return new AuthGuard(config, workspace);
}

function cloudGuard(): AuthGuard {
  return guardWith({ SYNC_JWT_PUBLIC_KEY: signingKeys.publicKey });
}

function contextFor(token: string): {
  context: ExecutionContext;
  request: { headers: Record<string, string>; user?: UserContext };
} {
  const request: { headers: Record<string, string>; user?: UserContext } = {
    headers: { authorization: `Bearer ${token}` },
  };
  const context = {
    switchToHttp: () => ({ getRequest: () => request }),
  } as unknown as ExecutionContext;
  return { context, request };
}

function sign(
  payload: Record<string, unknown>,
  privateKey: string = signingKeys.privateKey,
  expiresIn = 300,
): string {
  return jwt.sign(payload, privateKey, { algorithm: "RS256", expiresIn });
}

function syncToken(overrides: Record<string, unknown> = {}): string {
  return sign({
    sub: USER_ID,
    prefix: `users/${USER_ID}/`,
    profileLimit: 5,
    scope: "sync",
    ...overrides,
  });
}

describe("AuthGuard", () => {
  let warnSpy: jest.SpyInstance;

  beforeEach(() => {
    warnSpy = jest
      .spyOn(Logger.prototype, "warn")
      .mockImplementation(() => undefined);
    jest.spyOn(Logger.prototype, "log").mockImplementation(() => undefined);
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("accepts a sync token and scopes the request to its own namespace", async () => {
    const { context, request } = contextFor(syncToken());

    await expect(cloudGuard().canActivate(context)).resolves.toBe(true);
    expect(request.user).toEqual({
      mode: "cloud",
      prefix: `users/${USER_ID}/`,
      profileLimit: 5,
      sub: USER_ID,
    });
  });

  it("refuses an account access token signed with the same key", async () => {
    const accessToken = sign({
      sub: USER_ID,
      email: "owner@example.com",
      sid: "device-1",
    });
    const { context, request } = contextFor(accessToken);

    await expect(cloudGuard().canActivate(context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
    expect(request.user).toBeUndefined();
    expect(warnSpy).toHaveBeenCalled();
  });

  it("refuses a per-profile agent token signed with the same key", async () => {
    const agentToken = sign({
      sub: USER_ID,
      email: "owner@example.com",
      sid: "agent:run-1",
      scope: "agent",
      profile: "profile-1",
    });
    const { context, request } = contextFor(agentToken);

    await expect(cloudGuard().canActivate(context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
    expect(request.user).toBeUndefined();
  });

  it.each([
    ["an array", ["sync"]],
    ["another case", "SYNC"],
    ["an empty string", ""],
    ["null", null],
  ])("refuses a scope that is %s", async (_label, scope) => {
    const { context } = contextFor(syncToken({ scope }));

    await expect(cloudGuard().canActivate(context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
  });

  it("refuses a sync token signed with a different key", async () => {
    const forged = sign(
      {
        sub: USER_ID,
        prefix: `users/${USER_ID}/`,
        profileLimit: 5,
        scope: "sync",
      },
      otherKeys.privateKey,
    );
    const { context } = contextFor(forged);

    await expect(cloudGuard().canActivate(context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
  });

  it("refuses an expired sync token", async () => {
    const expired = sign(
      {
        sub: USER_ID,
        prefix: `users/${USER_ID}/`,
        profileLimit: 5,
        scope: "sync",
      },
      signingKeys.privateKey,
      -10,
    );
    const { context } = contextFor(expired);

    await expect(cloudGuard().canActivate(context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
  });

  it("refuses a sync token whose prefix is not a single user namespace", async () => {
    const { context } = contextFor(syncToken({ prefix: "users/" }));

    await expect(cloudGuard().canActivate(context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
  });

  it("still accepts the configured token of a self-hosted server", async () => {
    const guard = guardWith({
      SYNC_TOKEN: SELF_HOSTED_TOKEN,
      SYNC_JWT_PUBLIC_KEY: signingKeys.publicKey,
    });
    const { context, request } = contextFor(SELF_HOSTED_TOKEN);

    await expect(guard.canActivate(context)).resolves.toBe(true);
    expect(request.user).toEqual({
      mode: "self-hosted",
      prefix: "",
      profileLimit: 0,
      role: "owner",
    });
  });

  it("accepts a member's own token and names the member", async () => {
    const config = { get: () => undefined } as unknown as ConfigService;
    const member = { id: "m1", name: "Olga", role: "member" };
    const workspace = {
      store: {
        memberByToken: (token: string) =>
          token === "member-token" ? member : null,
        apiKeyByToken: () => null,
      },
    } as unknown as WorkspaceService;
    const guard = new AuthGuard(config, workspace);

    const { context, request } = contextFor("member-token");
    await expect(guard.canActivate(context)).resolves.toBe(true);
    expect(request.user).toEqual({
      mode: "self-hosted",
      prefix: "",
      profileLimit: 0,
      sub: "m1",
      role: "member",
      name: "Olga",
    });

    const refused = contextFor("someone-elses-token");
    await expect(guard.canActivate(refused.context)).rejects.toBeInstanceOf(
      UnauthorizedException,
    );
  });
});
