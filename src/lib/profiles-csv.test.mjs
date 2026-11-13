import assert from "node:assert/strict";
import test from "node:test";
import {
  csvDate,
  csvField,
  PROFILE_CSV_COLUMNS,
  profileCsvRow,
  profilesToCsv,
} from "./profiles-csv.ts";

const lookups = {
  groups: [{ id: "g1", name: "Shops" }],
  statuses: [{ id: "s1", name: "Warming up" }],
  proxies: [
    {
      id: "px1",
      name: "DE residential",
      proxy_settings: {
        proxy_type: "http",
        host: "1.2.3.4",
        port: 8080,
        username: "u",
        password: "secret",
      },
    },
  ],
  vpns: [{ id: "v1", name: "Office WG" }],
};
const headers = Object.fromEntries(PROFILE_CSV_COLUMNS.map((c) => [c, c]));
const profile = (over) => ({
  id: "p",
  name: "Профиль, один",
  browser: "camoufox",
  version: "1",
  release_type: "stable",
  ...over,
});

test("a field is quoted when it holds a comma, quote or line break", () => {
  assert.equal(csvField("plain"), "plain");
  assert.equal(csvField("a,b"), '"a,b"');
  assert.equal(csvField('say "hi"'), '"say ""hi"""');
  assert.equal(csvField("two\nlines"), '"two\nlines"');
});

test("a cell a spreadsheet would run as a formula is defused", () => {
  assert.equal(csvField("=SUM(A1)"), "'=SUM(A1)");
  assert.equal(csvField("+7 999"), "'+7 999");
  assert.equal(csvField("-1"), "'-1");
  assert.equal(csvField("@cmd"), "'@cmd");
});

test("a row names the group, status, proxy and engine and never the proxy login", () => {
  const row = profileCsvRow(
    profile({
      group_id: "g1",
      status_id: "s1",
      proxy_id: "px1",
      tags: ["a", "b"],
      note: "n",
    }),
    lookups,
  );
  assert.equal(row.group, "Shops");
  assert.equal(row.status, "Warming up");
  assert.equal(row.proxyName, "DE residential");
  assert.equal(row.proxyAddress, "1.2.3.4:8080");
  assert.equal(row.tags, "a, b");
  assert.equal(row.engine, "Camoufox");
  assert.ok(!Object.values(row).some((cell) => cell.includes("secret")));
});

test("a profile on a VPN shows its name and no address; a bare one is empty", () => {
  const vpn = profileCsvRow(profile({ vpn_id: "v1" }), lookups);
  assert.equal(vpn.proxyName, "Office WG");
  assert.equal(vpn.proxyAddress, "");
  const bare = profileCsvRow(profile({ proxy_id: "gone" }), lookups);
  assert.equal(bare.proxyName, "");
  assert.equal(bare.group, "");
});

test("dates are local and empty when unknown", () => {
  assert.equal(csvDate(undefined), "");
  assert.equal(csvDate(0), "");
  const seconds = Math.floor(new Date(2026, 10, 14, 9, 5).getTime() / 1000);
  assert.equal(csvDate(seconds), "2026-11-14 09:05");
});

test("the file has a header, a CRLF line per profile and a closing break", () => {
  const text = profilesToCsv(
    [profile({}), profile({ name: "B" })],
    lookups,
    headers,
  );
  const lines = text.split("\r\n");
  assert.equal(lines.length, 4);
  assert.equal(lines[0], PROFILE_CSV_COLUMNS.join(","));
  assert.ok(lines[1].startsWith('"Профиль, один",'));
  assert.equal(lines[3], "");
  assert.equal(lines[1].split(",").length > 9, true);
});
