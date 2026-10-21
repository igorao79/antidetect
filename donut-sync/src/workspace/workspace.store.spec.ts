import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WorkspaceStore } from "./workspace.store.js";

describe("WorkspaceStore", () => {
  let dir: string;
  let store: WorkspaceStore;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "owl-store-"));
    store = new WorkspaceStore(dir);
  });

  afterEach(() => {
    store.close();
    rmSync(dir, { recursive: true, force: true });
  });

  it("makes the first owner from the owner invite, once", () => {
    expect(store.hasOwner()).toBe(false);
    const invite = store.createInvite("owner", null);
    const joined = store.join(invite.code, "Ivan");
    expect(joined?.member).toMatchObject({ name: "Ivan", role: "owner" });
    expect(store.hasOwner()).toBe(true);
    expect(store.join(invite.code, "Someone")).toBeNull();
    // A second owner invite cannot make a second owner.
    const late = store.createInvite("owner", null);
    expect(store.join(late.code, "Mallory")).toBeNull();
  });

  it("only the newest owner invite is valid", () => {
    const first = store.createInvite("owner", null);
    const second = store.createInvite("owner", null);
    expect(store.join(first.code, "Old link")).toBeNull();
    expect(store.join(second.code, "Ivan")?.member.role).toBe("owner");
  });

  it("knows a member by token until the member is removed", () => {
    const owner = store.join(store.createInvite("owner", null).code, "Ivan");
    const invite = store.createInvite("member", owner?.member.id ?? null);
    const member = store.join(invite.code, "Olga");
    expect(member).not.toBeNull();
    if (!member) return;
    expect(store.memberByToken(member.token)?.name).toBe("Olga");
    expect(store.memberByToken("not-a-token")).toBeNull();
    expect(store.listMembers().map((m) => m.name)).toEqual(["Ivan", "Olga"]);

    expect(store.removeMember(member.member.id)).toBe("removed");
    expect(store.memberByToken(member.token)).toBeNull();
    expect(store.removeMember(member.member.id)).toBe("not_found");
    expect(store.removeMember(owner?.member.id ?? "")).toBe("owner");
  });

  it("lists open member invites and revokes them", () => {
    const invite = store.createInvite("member", null);
    expect(store.listInvites().map((i) => i.id)).toEqual([invite.id]);
    expect(store.revokeInvite(invite.id)).toBe(true);
    expect(store.listInvites()).toEqual([]);
    expect(store.join(invite.code, "Late")).toBeNull();
  });

  it("keeps the first uploader of a profile as its sharer", () => {
    store.recordProfileOwner("p1", "m1");
    store.recordProfileOwner("p1", "m2");
    expect(store.profileOwner("p1")).toBe("m1");
    expect(store.profileOwner("p2")).toBeNull();
  });

  it("stores neither tokens nor codes in the clear", () => {
    const invite = store.createInvite("owner", null);
    const joined = store.join(invite.code, "Ivan");
    store.close();
    const { DatabaseSync } = require("node:sqlite");
    const db = new DatabaseSync(join(dir, "owl.db"));
    const dump = JSON.stringify([
      db.prepare("SELECT * FROM members").all(),
      db.prepare("SELECT * FROM invites").all(),
    ]);
    db.close();
    expect(dump).not.toContain(invite.code);
    expect(dump).not.toContain(joined?.token);
    store = new WorkspaceStore(dir);
  });
});
