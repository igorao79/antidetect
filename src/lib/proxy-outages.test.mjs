import assert from "node:assert/strict";
import test from "node:test";
import {
  applyHealthEvent,
  NOTIFY_MAX_NAMES,
  outageNotice,
  outageRows,
  pruneStopped,
} from "./proxy-outages.ts";

const running = () => true;
const stopped = () => false;
const down = (id, at = 100, code = "PROXY_UPSTREAM_TIMEOUT") => ({
  profile_id: id,
  status: "down",
  at,
  code,
});

test("a down verdict opens an outage and a repeat changes nothing", () => {
  const once = applyHealthEvent(new Map(), down("a"), running);
  assert.deepEqual(once.get("a"), {
    profileId: "a",
    code: "PROXY_UPSTREAM_TIMEOUT",
    since: 100,
    cut: false,
  });
  assert.equal(applyHealthEvent(once, down("a", 105), running), once);
});

test("a new reason keeps the outage's start", () => {
  const once = applyHealthEvent(new Map(), down("a"), running);
  const next = applyHealthEvent(
    once,
    down("a", 110, "PROXY_AUTH_FAILED"),
    running,
  );
  assert.equal(next.get("a").code, "PROXY_AUTH_FAILED");
  assert.equal(next.get("a").since, 100);
  assert.notEqual(next, once);
});

test("ok and degraded end an outage, and never open one", () => {
  const once = applyHealthEvent(new Map(), down("a"), running);
  for (const status of ["ok", "degraded"]) {
    const event = { profile_id: "a", status, at: 120 };
    assert.equal(applyHealthEvent(once, event, running).size, 0);
    const empty = new Map();
    assert.equal(applyHealthEvent(empty, event, running), empty);
  }
});

test("cleared ends the outage of a stopped profile", () => {
  const once = applyHealthEvent(new Map(), down("a"), running);
  const cleared = { profile_id: "a", cleared: true };
  assert.equal(applyHealthEvent(once, cleared, stopped).size, 0);
});

test("cleared while the browser runs marks the session cut", () => {
  const once = applyHealthEvent(new Map(), down("a"), running);
  const cleared = { profile_id: "a", cleared: true };
  const cut = applyHealthEvent(once, cleared, running);
  assert.equal(cut.get("a").cut, true);
  assert.equal(applyHealthEvent(cut, cleared, running), cut);
  const empty = new Map();
  assert.equal(applyHealthEvent(empty, cleared, running), empty);
});

test("a profile that leaves the running set loses its outage", () => {
  const outages = applyHealthEvent(
    applyHealthEvent(new Map(), down("a"), running),
    down("b"),
    running,
  );
  const next = pruneStopped(outages, new Set(["a", "b"]), new Set(["b"]));
  assert.deepEqual([...next.keys()], ["b"]);
  assert.equal(
    pruneStopped(outages, new Set(["a", "b"]), new Set(["a", "b"])),
    outages,
  );
  // Never seen running (as in the E2E harness): nothing to prune.
  assert.equal(pruneStopped(outages, new Set(), new Set()), outages);
});

test("rows name known profiles, oldest outage first", () => {
  let outages = new Map();
  outages = applyHealthEvent(outages, down("b", 200), running);
  outages = applyHealthEvent(outages, down("a", 100), running);
  outages = applyHealthEvent(outages, down("gone", 50), running);
  const rows = outageRows(outages, [
    { id: "a", name: "Alpha" },
    { id: "b", name: "Beta" },
  ]);
  assert.deepEqual(
    rows.map((row) => row.name),
    ["Alpha", "Beta"],
  );
});

test("one profile gets a notice by name", () => {
  const rows = [{ profileId: "a", name: "Alpha", code: "X", cut: false }];
  assert.deepEqual(outageNotice(new Set(["a"]), rows), {
    kind: "one",
    name: "Alpha",
    code: "X",
  });
});

test("several profiles of one window get one grouped notice", () => {
  const rows = ["A", "B", "C", "D", "E"].map((name) => ({
    profileId: name.toLowerCase(),
    name,
    code: "PROXY_UPSTREAM_TIMEOUT",
    cut: false,
  }));
  const notice = outageNotice(new Set(["a", "b", "c", "d", "e"]), rows);
  assert.equal(notice.kind, "many");
  assert.equal(notice.count, 5);
  assert.equal(notice.names.length, NOTIFY_MAX_NAMES);
  assert.equal(notice.more, 5 - NOTIFY_MAX_NAMES);
  assert.equal(notice.code, "PROXY_UPSTREAM_TIMEOUT");
});

test("a grouped notice names a reason only when they share one", () => {
  const rows = [
    { profileId: "a", name: "A", code: "X", cut: false },
    { profileId: "b", name: "B", code: "Y", cut: false },
  ];
  const notice = outageNotice(new Set(["a", "b"]), rows);
  assert.equal(notice.code, undefined);
  assert.equal(notice.more, 0);
});

test("no notice when the batch recovered before the window closed", () => {
  const rows = [{ profileId: "x", name: "X", cut: false }];
  assert.equal(outageNotice(new Set(["a"]), rows), null);
});
