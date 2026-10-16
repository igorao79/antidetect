from scn import *
wait_api()
LOG = os.path.join(HERE, "proxy.log")
proxy = subprocess.Popen(["node", "test-proxy.mjs", "18765", LOG], cwd=HERE); time.sleep(1.5)
status, r = api("POST", "/api/v2/automation/proxies", {"title": f"S5b local {int(time.time())}", "type": "http", "host": "127.0.0.1", "port": 18765})
pid = r["data"]["uuid"]; created = []
try:
    for browser in ("camoufox",):
        created.append(create(f"S5b {browser}", "win", browser=browser, proxy=pid))
    proxy.terminate(); proxy.wait(5); time.sleep(1)   # the proxy dies after the profiles exist
    for u, browser in zip(created, ("camoufox",)):
        status, st = start(u)
        print(browser, "start ->", status, str(st)[:140])
        if status != 200:
            check(f"{browser}: launch refused on a dead proxy", True); continue
        time.sleep(4)
        try:
            fp = probe(st, url="https://example.com", tries=1)
            loaded = fp["url"].startswith("https://example.com")
            check(f"{browser}: NO direct fallback when the proxy is dead (page must not load)", not loaded, fp["url"])
        except Exception as e:
            check(f"{browser}: NO direct fallback when the proxy is dead (page must not load)", True, str(e)[:120])
        stop(u)
finally:
    if proxy.poll() is None: proxy.terminate()
    for u in created: stop(u)
    time.sleep(2); delete(*created)
    api("DELETE", "/api/v2/automation/proxies", {"uuids": [pid]})
summary()
