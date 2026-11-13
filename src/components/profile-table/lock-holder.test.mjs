import assert from "node:assert/strict";
import test from "node:test";
import { holderInitials } from "./lock-holder.ts";

test("a name gives the first letters of its first two words", () => {
  assert.equal(holderInitials("Anna Petrova"), "AP");
  assert.equal(holderInitials("anna maria petrova"), "AM");
  assert.equal(holderInitials("Igor"), "I");
});

test("an email gives the letters of its local part", () => {
  assert.equal(holderInitials("anna.petrova@example.com"), "AP");
  assert.equal(holderInitials("bob@example.com"), "B");
});

test("nothing to read gives a question mark", () => {
  assert.equal(holderInitials(""), "?");
  assert.equal(holderInitials(undefined), "?");
  assert.equal(holderInitials("  @example.com"), "?");
});
