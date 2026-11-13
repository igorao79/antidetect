import assert from "node:assert/strict";
import test from "node:test";
import {
  likelyLanguage,
  orderProxyChoices,
  sharedProxyId,
} from "./proxy-choices.ts";

const list = [
  { id: "c", name: "Charlie" },
  { id: "a", name: "alpha" },
  { id: "b", name: "Bravo 10" },
  { id: "d", name: "Bravo 2" },
];

test("puts the current proxy first and sorts the rest by name", () => {
  const ids = orderProxyChoices(list, "c").map((p) => p.id);
  assert.deepEqual(ids, ["c", "a", "d", "b"]);
});

test("sorts alphabetically when there is no current proxy", () => {
  const ids = orderProxyChoices(list, null).map((p) => p.id);
  assert.deepEqual(ids, ["a", "d", "b", "c"]);
});

test("does not mutate its input", () => {
  const copy = list.map((p) => ({ ...p }));
  orderProxyChoices(list, "b");
  assert.deepEqual(list, copy);
});

test("an unknown current id changes nothing about the order", () => {
  assert.equal(orderProxyChoices(list, "zzz").length, list.length);
});

test("sharedProxyId needs every profile on the same proxy", () => {
  assert.equal(sharedProxyId(["x", "x"]), "x");
  assert.equal(sharedProxyId(["x", "y"]), null);
  assert.equal(sharedProxyId(["x", null]), null);
  assert.equal(sharedProxyId([]), null);
});

test("likelyLanguage reads the main language of a country", () => {
  assert.equal(likelyLanguage("DE"), "de");
  assert.equal(likelyLanguage("jp"), "ja");
  assert.equal(likelyLanguage("US"), "en");
  assert.equal(likelyLanguage(""), null);
  assert.equal(likelyLanguage("USA"), null);
});
