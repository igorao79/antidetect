import assert from "node:assert/strict";
import test from "node:test";
import { proxiesToCheck, runPool } from "./bulk-proxy-check.ts";

test("the pool never runs more than the limit at once and finishes all", async () => {
  let active = 0;
  let peak = 0;
  const done = [];
  await runPool([1, 2, 3, 4, 5, 6, 7], 3, async (item) => {
    active += 1;
    peak = Math.max(peak, active);
    await new Promise((resolve) => setTimeout(resolve, 5));
    active -= 1;
    done.push(item);
  });
  assert.equal(peak, 3);
  assert.deepEqual(done.sort(), [1, 2, 3, 4, 5, 6, 7]);
});

test("an empty list resolves at once", async () => {
  await runPool([], 4, async () => assert.fail("no work"));
});

test("a proxy shared by profiles is checked once, VPN profiles are left out", () => {
  const stored = [{ id: "p1" }, { id: "p2" }, { id: "p3" }];
  const profiles = [
    { proxy_id: "p1" },
    { proxy_id: "p1" },
    { proxy_id: "p2", vpn_id: "v" },
    { proxy_id: null },
    {},
  ];
  assert.deepEqual(proxiesToCheck(profiles, stored), [{ id: "p1" }]);
});
