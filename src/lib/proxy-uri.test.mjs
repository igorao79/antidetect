import assert from "node:assert/strict";
import test from "node:test";
import { formatProxyUri } from "./proxy-uri.ts";

test("plain proxy without credentials", () => {
  assert.equal(
    formatProxyUri({ proxy_type: "http", host: "1.2.3.4", port: 8080 }),
    "http://1.2.3.4:8080",
  );
});

test("credentials are percent-encoded", () => {
  assert.equal(
    formatProxyUri({
      proxy_type: "socks5",
      host: "h.example.com",
      port: 1080,
      username: "us er@x",
      password: "p:a/ss#",
    }),
    "socks5://us%20er%40x:p%3Aa%2Fss%23@h.example.com:1080",
  );
});

test("username only", () => {
  assert.equal(
    formatProxyUri({ proxy_type: "http", host: "h", port: 1, username: "u" }),
    "http://u@h:1",
  );
});

test("ipv6 host is bracketed", () => {
  assert.equal(
    formatProxyUri({ proxy_type: "http", host: "::1", port: 3128 }),
    "http://[::1]:3128",
  );
});

test("share link types return the link", () => {
  assert.equal(
    formatProxyUri({
      proxy_type: "vless",
      host: "a",
      port: 443,
      vless_uri: "vless://id@a:443?security=tls",
    }),
    "vless://id@a:443?security=tls",
  );
});
