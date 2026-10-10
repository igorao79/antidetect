from scn import *
wait_api()
if os.path.exists("s1-ids.json"):
    old = json.load(open("s1-ids.json"))
    for u in old.values(): stop(u)
    time.sleep(2); delete(*old.values())
ids = {os_name: create(f"S1 camoufox {os_name}", os_name) for os_name in ("win", "mac", "lin")}
json.dump(ids, open("s1-ids.json", "w"))
fps = {}
for os_name, uuid in ids.items():
    status, started = start(uuid)
    check(f"start {os_name}", status == 200 and started.get("state") == "STARTED", f"http {status}")
    if status != 200:
        print(started); continue
    time.sleep(4)
    fps[os_name] = probe(started, url="https://example.com")
    show(f"camoufox {os_name}", fps[os_name])
expect = {"win": ("Win32", "Windows NT"), "mac": ("MacIntel", "Macintosh"), "lin": ("Linux", "Linux")}
for os_name, fp in fps.items():
    plat, ua = expect[os_name]
    check(f"{os_name}: platform matches OS", plat in fp["platform"], fp["platform"])
    check(f"{os_name}: UA matches OS", ua in fp["userAgent"])
    check(f"{os_name}: UA names the real engine (Firefox/156)", "Firefox/156.0" in fp["userAgent"] and "rv:156.0" in fp["userAgent"])
    check(f"{os_name}: navigator.webdriver is false", fp["webdriver"] is False)
    check(f"{os_name}: page loaded through local proxy", fp["url"].startswith("https://example.com"), fp["url"])
# With no proxy the exit is this machine's own address. Set EXPECT_TZ / EXPECT_LANG
# (e.g. EXPECT_TZ=Europe/Berlin EXPECT_LANG=de) to assert the geolocation too.
my_ip = public_ip()
for os_name, fp in fps.items():
    if expected("EXPECT_TZ"):
        check(f"{os_name}: timezone follows the exit address", fp["timezone"] in expected("EXPECT_TZ"), fp["timezone"])
    if expected("EXPECT_LANG"):
        check(f"{os_name}: language follows the exit address", any(fp["languages"][0].startswith(l) for l in expected("EXPECT_LANG")), str(fp["languages"]))
    check(f"{os_name}: WebRTC shows no address other than the exit", all(c.split(":", 1)[1] == my_ip for c in fp["webrtc"] if c != "unavailable"), str(fp["webrtc"]))
tzs = {fp["timezone"] for fp in fps.values()}
check("all profiles on one route agree on the timezone", len(tzs) == 1, str(tzs))
hashes = [fp["canvasHash"] for fp in fps.values()]
check("canvas fingerprints differ between profiles", len(set(hashes)) == len(hashes), str(hashes))
gpus = [fp["webglRenderer"] for fp in fps.values()]
check("WebGL renderers are OS-appropriate", ("Direct3D" in (fps["win"]["webglRenderer"] or "")) and ("Direct3D" not in (fps["mac"]["webglRenderer"] or "")), str(gpus))
status, active = api("GET", "/api/profiles/active")
check("all three listed as active", len(active) == 3, str(len(active)))
summary()
