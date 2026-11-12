import assert from "node:assert/strict";
import test from "node:test";
import { isValidProxyHost } from "./proxy-host.ts";

test("names and addresses pass", () => {
  for (const host of [
    "1.2.3.4",
    "proxy.example.com",
    "прокси.рф",
    "host_1-a.io",
    "2001:db8::1",
    " [2001:db8::1] ",
  ]) {
    assert.equal(isValidProxyHost(host), true, host);
  }
});

test("anything that is not a host is refused", () => {
  for (const host of [
    "",
    "   ",
    "http://evil.com",
    "bad host name",
    "host/path",
    "1.2.3.4:8080",
    "<script>",
    "a@b",
    "[nope]",
    ".x",
    "x.",
  ]) {
    assert.equal(isValidProxyHost(host), false, host);
  }
});
