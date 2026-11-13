import assert from "node:assert/strict";
import test from "node:test";
import {
  allPinned,
  parsePinned,
  pinnedFirst,
  serializePinned,
  withPinned,
} from "./pinned-profiles.ts";

test("a stored value that is not a list of ids reads as nothing pinned", () => {
  assert.equal(parsePinned(null).size, 0);
  assert.equal(parsePinned("not json").size, 0);
  assert.equal(parsePinned('{"a":1}').size, 0);
  assert.deepEqual([...parsePinned('["a",3,"b"]')], ["a", "b"]);
});

test("ids survive a round trip", () => {
  const pinned = new Set(["a", "b"]);
  assert.deepEqual([...parsePinned(serializePinned(pinned))], ["a", "b"]);
});

test("pinning returns a new set and leaves the old one alone", () => {
  const before = new Set(["a"]);
  const after = withPinned(before, ["b", "c"], true);
  assert.deepEqual([...before], ["a"]);
  assert.deepEqual([...after].sort(), ["a", "b", "c"]);
  assert.deepEqual([...withPinned(after, ["a", "z"], false)].sort(), [
    "b",
    "c",
  ]);
});

test("a change that changes nothing returns the same set", () => {
  const before = new Set(["a"]);
  assert.equal(withPinned(before, ["a"], true), before);
  assert.equal(withPinned(before, ["x"], false), before);
});

test("allPinned is false for an empty list", () => {
  assert.equal(allPinned(new Set(["a"]), []), false);
  assert.equal(allPinned(new Set(["a", "b"]), ["a", "b"]), true);
  assert.equal(allPinned(new Set(["a"]), ["a", "b"]), false);
});

test("pinned rows rise above the sort and keep their order", () => {
  const rows = ["a", "b", "c", "d", "e"];
  assert.deepEqual(
    pinnedFirst(rows, (row) => row === "d" || row === "b"),
    ["b", "d", "a", "c", "e"],
  );
  assert.deepEqual(
    pinnedFirst(rows, () => false),
    rows,
  );
});
