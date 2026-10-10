from scn import *
wait_api()
LOG = os.path.join(HERE, "proxy.log")
if os.path.exists(LOG): os.remove(LOG)
proxy = subprocess.Popen(["node", "test-proxy.mjs", "18765", LOG], cwd=HERE)
time.sleep(1.5)
created = []
try:
    status, r = api("POST", "/api/v2/automation/proxies", {"title": f"S5 local {int(time.time())}", "type": "http", "host": "127.0.0.1", "port": 18765})
    check("create proxy", status == 200 and r.get("success"), f"{status} {str(r)[:120]}")
    pid = r["data"]["uuid"]
    status, lst = api("GET", "/api/v2/automation/proxies")
    check("proxy is listed", any(p["uuid"] == pid for p in lst["data"]))
    for browser in ("camoufox", "cloakbrowser"):
        u = create(f"S5 {browser} via proxy", "win", browser=browser, proxy=pid); created.append(u)
        before = len(open(LOG).read().splitlines())
        status, st = start(u)
        check(f"{browser}: start with proxy", status == 200, f"{status} {str(st)[:160]}")
        if status != 200: continue
        time.sleep(4)
        fp = probe(st, url="https://example.com")
        lines = open(LOG).read().splitlines()[before:]
        check(f"{browser}: page request went through the assigned proxy", any("example.com" in l for l in lines), f"{len(lines)} proxied requests")
        check(f"{browser}: page loaded", fp["url"].startswith("https://example.com"), fp["url"])
        show(f"{browser} via proxy", fp, keys=("timezone", "languages", "webrtc"))
        stop(u)
    # A profile cannot be created on a proxy that is down (see s5b for a proxy
    # that dies after the profile exists).
    proxy.terminate(); proxy.wait(5); time.sleep(1)
    status, r = api("POST", "/api/v2/automation/profiles", {"title": "S5 dead proxy", "fingerprint": {"os": "win"}, "proxy": {"uuid": pid}})
    check("creating a profile on a dead proxy is refused", status == 400, f"{status} {str(r)[:140]}")
    if status == 200: created.append(r["data"]["uuid"])
finally:
    if proxy.poll() is None: proxy.terminate()
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    api("DELETE", "/api/v2/automation/proxies", {"uuids": [pid]})
summary()
