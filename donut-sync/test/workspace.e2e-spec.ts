import { mkdtempSync, rmSync } from "node:fs";
import type { Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { INestApplication } from "@nestjs/common";
import { ConfigModule } from "@nestjs/config";
import type { NestExpressApplication } from "@nestjs/platform-express";
import { Test, TestingModule } from "@nestjs/testing";
import { AppController } from "./../src/app.controller.js";
import { AppService } from "./../src/app.service.js";
import { configureBodyParsing } from "./../src/http-setup.js";
import { SyncModule } from "./../src/sync/sync.module.js";
import { WorkspaceService } from "./../src/workspace/workspace.service.js";

/**
 * A server the way `docker compose up` runs it: members who join by invite,
 * objects on the server's own disk, no S3 and no SYNC_TOKEN.
 */
describe("Owl server with members (disk storage)", () => {
  let app: INestApplication;
  let base: string;
  let dataDir: string;
  const saved = { ...process.env };

  const call = async (
    method: string,
    path: string,
    token?: string,
    body?: unknown,
  ) => {
    const response = await fetch(`${base}${path}`, {
      method,
      headers: {
        ...(token ? { authorization: `Bearer ${token}` } : {}),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    return {
      status: response.status,
      body: text ? (JSON.parse(text) as Record<string, unknown>) : {},
    };
  };

  beforeAll(async () => {
    dataDir = mkdtempSync(join(tmpdir(), "owl-e2e-"));
    for (const name of [
      "SYNC_TOKEN",
      "SYNC_JWT_PUBLIC_KEY",
      "S3_ENDPOINT",
      "PUBLIC_URL",
    ]) {
      delete process.env[name];
    }
    process.env.STORAGE = "disk";
    process.env.DATA_DIR = dataDir;
    process.env.WORKSPACE_NAME = "Team";

    const moduleFixture: TestingModule = await Test.createTestingModule({
      imports: [ConfigModule.forRoot({ isGlobal: true }), SyncModule],
      controllers: [AppController],
      providers: [AppService],
    }).compile();
    app = moduleFixture.createNestApplication<NestExpressApplication>({
      bodyParser: false,
    });
    configureBodyParsing(app as NestExpressApplication);
    await app.listen(0);
    const port = ((app.getHttpServer() as Server).address() as AddressInfo)
      .port;
    base = `http://127.0.0.1:${port}`;
    process.env.PUBLIC_URL = base;
  });

  afterAll(async () => {
    await app.close();
    rmSync(dataDir, { recursive: true, force: true });
    process.env = saved;
  });

  const ownerCode = () =>
    app.get(WorkspaceService).store.createInvite("owner", null).code;

  let owner: string;
  let member: string;
  let memberId: string;

  it("lets the owner and then an invited member join", async () => {
    const refused = await call("POST", "/v1/join", undefined, {
      code: "made-up",
      name: "Eve",
    });
    expect(refused).toEqual({ status: 401, body: { error: "invite_invalid" } });

    const code = ownerCode();
    const joined = await call("POST", "/v1/join", undefined, {
      code,
      name: "  Ivan ",
    });
    expect(joined.status).toBe(200);
    expect(joined.body).toMatchObject({
      member: { name: "Ivan", role: "owner" },
      workspace: { name: "Team" },
    });
    owner = joined.body.token as string;
    const reused = await call("POST", "/v1/join", undefined, {
      code,
      name: "Ivan again",
    });
    expect(reused.status).toBe(401);

    const invite = await call("POST", "/v1/invites", owner, { role: "member" });
    expect(invite.status).toBe(200);
    const link = new URL(invite.body.link as string);
    expect(link.protocol).toBe("owl:");
    expect(link.searchParams.get("server")).toBe(base);
    expect(link.searchParams.get("code")).toBe(invite.body.code);
    const listed = await call("GET", "/v1/invites", owner);
    expect(listed.body).toEqual({
      invites: [
        expect.objectContaining({
          id: invite.body.id,
          expiresAt: expect.any(String),
        }),
      ],
    });

    const joinedMember = await call("POST", "/v1/join", undefined, {
      code: invite.body.code,
      name: "Olga",
    });
    expect(joinedMember.body).toMatchObject({ member: { role: "member" } });
    member = joinedMember.body.token as string;
    memberId = (joinedMember.body.member as { id: string }).id;

    const me = await call("GET", "/v1/me", member);
    expect(me.body).toEqual({
      member: { id: memberId, name: "Olga", role: "member" },
      workspace: { name: "Team" },
    });
    const members = await call("GET", "/v1/members", member);
    expect(
      (members.body.members as Array<{ name: string }>).map((m) => m.name),
    ).toEqual(["Ivan", "Olga"]);
  });

  it("leaves invites and member management to the owner", async () => {
    expect((await call("POST", "/v1/invites", member, {})).status).toBe(403);
    expect((await call("GET", "/v1/invites", member)).status).toBe(403);
    expect((await call("GET", "/v1/members")).status).toBe(401);
  });

  it("keeps objects on disk behind signed URLs", async () => {
    const upload = await call("POST", "/v1/objects/presign-upload", member, {
      key: "profiles/p1/files/cookies.sqlite",
      metadata: { "updated-at": "1700000000" },
    });
    expect(upload.body.metadata).toEqual({ "updated-at": "1700000000" });
    const put = await fetch(upload.body.url as string, {
      method: "PUT",
      headers: {
        "content-type": "application/octet-stream",
        "x-amz-meta-updated-at": "1700000000",
      },
      body: "cookie bytes",
    });
    expect(put.status).toBe(200);

    // Bytes labelled JSON that are not JSON (an encrypted file) go through.
    const opaque = await call("POST", "/v1/objects/presign-upload", member, {
      key: "profiles/p1/files/sealed.bin",
      contentType: "application/json",
    });
    expect(
      (
        await fetch(opaque.body.url as string, {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: "not json at all",
        })
      ).status,
    ).toBe(200);

    // A JSON object keeps its exact bytes, spacing included.
    const json = '{ "name":  "Shop" }';
    const jsonUpload = await call(
      "POST",
      "/v1/objects/presign-upload",
      member,
      {
        key: "profiles/p1/metadata.json",
        contentType: "application/json",
      },
    );
    expect(
      (
        await fetch(jsonUpload.body.url as string, {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: json,
        })
      ).status,
    ).toBe(200);

    const stat = await call("POST", "/v1/objects/stat", owner, {
      key: "profiles/p1/files/cookies.sqlite",
    });
    expect(stat.body).toMatchObject({
      exists: true,
      size: 12,
      metadata: { "updated-at": "1700000000" },
    });

    const download = await call("POST", "/v1/objects/presign-download", owner, {
      key: "profiles/p1/metadata.json",
    });
    const got = await fetch(download.body.url as string);
    expect(await got.text()).toBe(json);

    const tampered = new URL(download.body.url as string);
    tampered.searchParams.set("sig", "0".repeat(64));
    expect((await fetch(tampered)).status).toBe(403);
    const wrongOp = new URL(download.body.url as string);
    expect(
      (await fetch(wrongOp, { method: "PUT", body: "overwrite" })).status,
    ).toBe(403);

    const list = await call("POST", "/v1/objects/list", owner, {
      prefix: "profiles/",
    });
    expect(
      (list.body.objects as Array<{ key: string }>).map((o) => o.key),
    ).toEqual([
      "profiles/p1/files/cookies.sqlite",
      "profiles/p1/files/sealed.bin",
      "profiles/p1/metadata.json",
    ]);
  });

  it("lets only the sharer or the owner take a profile off the server", async () => {
    const upload = await call("POST", "/v1/objects/presign-upload", owner, {
      key: "profiles/p2/manifest.json",
    });
    await fetch(upload.body.url as string, { method: "PUT", body: "{}" });

    const refused = await call("POST", "/v1/objects/delete-prefix", member, {
      prefix: "profiles/p2/",
    });
    expect(refused.status).toBe(403);
    const wipe = await call("POST", "/v1/objects/delete-prefix", member, {
      prefix: "",
    });
    expect(wipe.status).toBe(403);

    const own = await call("POST", "/v1/objects/delete-prefix", member, {
      prefix: "profiles/p1/",
      tombstoneKey: "tombstones/profiles/p1.json",
    });
    expect(own.body).toEqual({ deletedCount: 3, tombstoneCreated: true });
    const byOwner = await call("POST", "/v1/objects/delete-prefix", owner, {
      prefix: "profiles/p2/",
    });
    expect(byOwner.body).toMatchObject({ deletedCount: 1 });
  });

  it("gives a profile to one member at a time", async () => {
    expect((await call("POST", "/v1/profile-locks/p3", member)).body).toEqual({
      success: true,
    });
    expect((await call("POST", "/v1/profile-locks/p3", owner)).body).toEqual({
      success: false,
      lockedBy: memberId,
      lockedByEmail: "Olga",
    });
    const listed = await call("GET", "/v1/profile-locks", owner);
    expect(listed.body).toEqual([
      expect.objectContaining({ profileId: "p3", lockedByEmail: "Olga" }),
    ]);
    expect(
      (await call("POST", "/v1/profile-locks/p3/heartbeat", owner)).body,
    ).toEqual({ refreshed: false });
    expect(
      (await call("POST", "/v1/profile-locks/p3/heartbeat", member)).body,
    ).toEqual({ refreshed: true });
    expect((await call("DELETE", "/v1/profile-locks/p3", owner)).status).toBe(
      404,
    );
    expect((await call("DELETE", "/v1/profile-locks/p3", member)).status).toBe(
      204,
    );
  });

  it("shuts a removed member out at once", async () => {
    expect(
      (await call("DELETE", `/v1/members/${memberId}`, member)).status,
    ).toBe(403);
    const ownerId = (
      (await call("GET", "/v1/me", owner)).body.member as {
        id: string;
      }
    ).id;
    expect((await call("DELETE", `/v1/members/${ownerId}`, owner)).status).toBe(
      400,
    );
    expect(
      (await call("DELETE", `/v1/members/${memberId}`, owner)).status,
    ).toBe(204);
    expect((await call("GET", "/v1/me", member)).status).toBe(401);
  });

  it("reports this server as its storage endpoint", async () => {
    const ready = await call("GET", "/readyz");
    expect(ready.body).toEqual({
      status: "ready",
      s3: true,
      storageEndpoint: base,
    });
  });
});
