from scn import *
wait_api()
if os.path.exists("s3-ids.json"):
    old = json.load(open("s3-ids.json"))
    for u in old.values(): stop(u)
    time.sleep(2); delete(*old.values())
ids = {os_name: create(f"S3 cloak {os_name}", os_name, browser="cloakbrowser") for os_name in ("win", "mac")}
json.dump(ids, open("s3-ids.json", "w"))
def deep(started, script="canvas-reads.js"):
    out = subprocess.run(["node", "cdp-eval.mjs", str(started["debug_port"]), script], cwd=HERE, capture_output=True, text=True, timeout=90)
    return json.loads(out.stdout)
fps, started_by, canv = {}, {}, {}
for os_name, uuid in ids.items():
    status, started = start(uuid)
    check(f"start cloakbrowser {os_name}", status == 200 and started.get("state") == "STARTED", str(started)[:200])
    if status != 200: continue
    started_by[os_name] = started
    check(f"{os_name}: ws_endpoint is a CDP browser endpoint", "/devtools/browser/" in started["ws_endpoint"])
    time.sleep(3)
    fps[os_name] = probe(started, url="https://example.com", set_cookie="cloak_session=xyz789")
    canv[os_name] = deep(started)
    show(f"cloakbrowser {os_name}", fps[os_name])
expect = {"win": ("Win32", "Windows NT", "Windows"), "mac": ("MacIntel", "Macintosh", "macOS")}
for os_name, fp in fps.items():
    plat, ua, uad = expect[os_name]
    check(f"{os_name}: platform matches OS", fp["platform"] == plat, fp["platform"])
    check(f"{os_name}: UA matches OS", ua in fp["userAgent"])
    check(f"{os_name}: client hints platform matches OS", fp.get("uaDataPlatform") == uad, str(fp.get("uaDataPlatform")))
    check(f"{os_name}: UA names the real engine (Chrome/145)", "Chrome/145." in fp["userAgent"])
    check(f"{os_name}: navigator.webdriver is false", fp["webdriver"] is False)
    if expected("EXPECT_TZ"):
        check(f"{os_name}: timezone follows the exit address", fp["timezone"] in expected("EXPECT_TZ"), fp["timezone"])
    if expected("EXPECT_LANG"):
        check(f"{os_name}: language follows the exit address", any(fp["languages"][0].startswith(l) for l in expected("EXPECT_LANG")), str(fp["languages"]))
check("getImageData noise differs between seeds", canv["win"]["getImageData"][0] != canv["mac"]["getImageData"][0], f'{canv["win"]["getImageData"][0]} vs {canv["mac"]["getImageData"][0]}')
check("getImageData noise is stable within a profile", canv["win"]["getImageData"][0] == canv["win"]["getImageData"][1])
if canv["win"]["toDataURL"] == canv["mac"]["toDataURL"]:
    note("toDataURL is identical across seeds: this build only noises getImageData/readPixels (known limit of the free v145 macOS build)")
else:
    note("toDataURL differs between seeds")
uuid = ids["win"]
status, r = stop(uuid); check("stop", status == 200, str(r)); time.sleep(2)
status, active = api("GET", "/api/profiles/active")
check("stopped profile leaves the active list", uuid not in [p["uuid"] for p in active])
status, restarted = start(uuid); check("restart", status == 200, str(restarted)[:160]); time.sleep(3)
after = probe(restarted, url="https://example.com"); canv_after = deep(restarted)
for key in ("userAgent", "platform", "screen", "cores", "memory", "webglRenderer", "timezone"):
    check(f"same {key} after restart", fps["win"][key] == after[key], f'{fps["win"][key]} -> {after[key]}')
check("same getImageData noise after restart", canv["win"]["getImageData"][0] == canv_after["getImageData"][0], f'{canv["win"]["getImageData"][0]} -> {canv_after["getImageData"][0]}')
check("cookie survived the restart", "cloak_session=xyz789" in after["cookie"], after["cookie"])
summary()
