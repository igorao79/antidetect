import assert from "node:assert/strict";
import test from "node:test";
import {
  diagnoseRouteFailure,
  diagnosisSentence,
  GENERIC_ROUTE_DIAGNOSIS,
} from "./proxy-check-diagnosis.ts";

const coded = (code) => JSON.stringify({ code });

test("every probe and launch code has its own reading", () => {
  const expected = {
    PROXY_UPSTREAM_TIMEOUT: "timeout",
    PROXY_UPSTREAM_UNREACHABLE: "unreachable",
    PROXY_AUTH_FAILED: "auth",
    PROXY_TLS_HANDSHAKE_FAILED: "tls",
    PROXY_PAYMENT_REQUIRED: "payment",
    PROXY_NOT_WORKING: "notWorking",
    PROFILE_PROXY_MISSING: "missing",
    PROFILE_VPN_MISSING: "missing",
    PROXY_HOST_INVALID: "config",
    VPN_UNREACHABLE: "unreachable",
  };
  for (const [code, kind] of Object.entries(expected)) {
    assert.equal(diagnoseRouteFailure(coded(code)).kind, kind, code);
  }
});

test("a coded error with context in front is still read", () => {
  const error = new Error(`Failed: ${coded("PROXY_UPSTREAM_TIMEOUT")}`);
  assert.equal(diagnoseRouteFailure(error).kind, "timeout");
});

test("a code that is not about the route is not explained", () => {
  assert.equal(diagnoseRouteFailure(coded("PROFILE_RUNNING")), null);
  assert.equal(
    diagnoseRouteFailure(coded("PROFILE_RUNNING"), { assumeRoute: true }),
    null,
  );
});

test("the backend's check sentences are read by their wording", () => {
  const read = (text) => diagnoseRouteFailure(text).kind;
  assert.equal(
    read("Connection refused by 1.2.3.4:80. Not accepting."),
    "refused",
  );
  assert.equal(read("Connection to a:1 timed out. Not responding."), "timeout");
  assert.equal(read("Could not resolve proxy host 'x'."), "dns");
  assert.equal(read("Proxy authentication failed for a:1."), "auth");
  assert.equal(read("Access denied by a:1 (403 Forbidden)."), "forbidden");
  assert.equal(read("Payment required by a:1 (402)."), "payment");
  assert.equal(read("Bad gateway from a:1 (502)."), "gateway");
  assert.equal(read("Proxy a:1 is temporarily unavailable (503)."), "gateway");
  assert.equal(read("Invalid proxy configuration for a:1."), "config");
  assert.equal(
    read("Proxy check failed for a:1. Could not connect through the proxy."),
    "unreachable",
  );
});

test("unknown text is explained only for a known route check", () => {
  assert.equal(diagnoseRouteFailure("something odd"), null);
  assert.equal(
    diagnoseRouteFailure("something odd", { assumeRoute: true }),
    GENERIC_ROUTE_DIAGNOSIS,
  );
  assert.equal(diagnoseRouteFailure("", { assumeRoute: true }), null);
});

test("the local-network caveat is left out where this computer cannot matter", () => {
  assert.equal(
    diagnoseRouteFailure(coded("PROXY_AUTH_FAILED")).localNetworkMatters,
    true,
  );
  assert.equal(
    diagnoseRouteFailure(coded("PROFILE_PROXY_MISSING")).localNetworkMatters,
    false,
  );
  assert.equal(
    diagnoseRouteFailure(coded("PROXY_PAYMENT_REQUIRED")).localNetworkMatters,
    false,
  );
});

test("the sentence joins meaning, advice and the local note", () => {
  const t = (key) => `<${key}>`;
  const timeout = diagnoseRouteFailure(coded("PROXY_UPSTREAM_TIMEOUT"));
  assert.equal(
    diagnosisSentence(t, timeout),
    "<routeDiagnosis.timeout.meaning> <routeDiagnosis.timeout.action> <routeDiagnosis.localNote>",
  );
  const missing = diagnoseRouteFailure(coded("PROFILE_PROXY_MISSING"));
  assert.equal(
    diagnosisSentence(t, missing),
    "<routeDiagnosis.missing.meaning> <routeDiagnosis.missing.action>",
  );
});
