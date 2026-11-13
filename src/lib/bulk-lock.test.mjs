import assert from "node:assert/strict";
import test from "node:test";
import { splitBulkLocked } from "./bulk-lock.ts";

const profiles = [
  { id: "a" },
  { id: "b", bulk_locked: true },
  { id: "c", bulk_locked: false },
  { id: "d", bulk_locked: true },
];

test("locked profiles are skipped and counted", () => {
  assert.deepEqual(splitBulkLocked(profiles, ["a", "b", "c", "d"]), {
    allowed: ["a", "c"],
    skipped: 2,
  });
});

test("nothing is skipped when nothing is locked", () => {
  assert.deepEqual(splitBulkLocked(profiles, ["a", "c"]), {
    allowed: ["a", "c"],
    skipped: 0,
  });
});

test("an empty selection stays empty", () => {
  assert.deepEqual(splitBulkLocked(profiles, []), { allowed: [], skipped: 0 });
});
