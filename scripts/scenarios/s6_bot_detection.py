from scn import *
wait_api()
created = [create(f"S6 {browser}", "win", browser=browser) for browser in ("camoufox",)]
by = {}
for uuid in created:
    status, started = start(uuid)
    if status == 200: by[started["browser"]] = started
time.sleep(4)
for browser, p in by.items():
    cmd = ["node", "bidi-eval.mjs", p["ws_endpoint"], "https://bot.sannysoft.com/", "sannysoft.js", "9000"]
    out = subprocess.run(cmd, cwd=HERE, capture_output=True, text=True, timeout=150)
    try:
        r = json.loads(out.stdout)
    except Exception:
        print(browser, "->", (out.stdout + out.stderr)[:300]); check(f"{browser}: sannysoft page read", False); continue
    print(f"--- {browser}: {r['passed']} passed, {len(r['failed'])} failed, {len(r['warn'])} warn of {r['total']} rows")
    for row in r["failed"]: print("    FAILED:", row["name"], "=", row["value"])
    for row in r["warn"]: print("    warn:", row["name"], "=", row["value"])
    # The "Chrome" rows test for window.chrome, which Firefox rightly lacks.
    failed = [row for row in r["failed"] if not (browser == "camoufox" and row["name"].startswith("Chrome"))]
    check(f"{browser}: sannysoft reports no failed test", not failed and r["passed"] >= 10, f"{r['passed']} passed")
for uuid in created: stop(uuid)
time.sleep(3); delete(*created)
summary()
