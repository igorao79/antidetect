"""SSH proxies: both engines route through an SSH server, and a wrong password is refused.

Needs the test server built first: `cd src-tauri && cargo build --example ssh_test_server`.
"""
from scn import *
wait_api()
SERVER = os.path.join(HERE, "..", "..", "src-tauri", "target", "debug", "examples", "ssh_test_server")
LOG = os.path.join(HERE, "ssh-proxy.log")
PORT = 18722

def forwards():
    return [l.split(" ", 1)[1] for l in open(LOG).read().splitlines() if l.startswith("forward ")]

def add_proxy(title, password):
    return api("POST", "/api/v2/automation/proxies", {"title": f"{title} {int(time.time())}", "type": "ssh", "host": "127.0.0.1", "port": PORT, "login": "tester", "password": password})

log = open(LOG, "w")
server = subprocess.Popen([SERVER, str(PORT), "tester", "s3cret"], stdout=log, stderr=subprocess.STDOUT)
time.sleep(1.5)
created, proxies = [], []
try:
    status, r = add_proxy("S7 ssh", "s3cret")
    check("create an SSH proxy", status == 200 and r.get("success"), f"{status} {str(r)[:120]}")
    pid = r["data"]["uuid"]; proxies.append(pid)
    for browser in ("camoufox", "cloakbrowser"):
        u = create(f"S7 {browser} via ssh", "win", browser=browser, proxy=pid); created.append(u)
        seen = len(forwards())
        status, st = start(u)
        check(f"{browser}: start with an SSH proxy", status == 200, f"{status} {str(st)[:160]}")
        if status != 200: continue
        time.sleep(4)
        fp = probe(st, url="https://example.com")
        new = forwards()[seen:]
        check(f"{browser}: HTTPS page loaded", fp["url"].startswith("https://example.com"), fp["url"])
        check(f"{browser}: HTTPS went through the SSH server", "example.com:443" in new, f"{len(new)} forwarded connections")
        if browser == "camoufox":
            # Plain HTTP takes its own path through the local proxy; Chromium
            # is served over SOCKS and upgrades to HTTPS anyway.
            seen = len(forwards())
            fp = probe(st, url="http://neverssl.com")
            new = forwards()[seen:]
            check("camoufox: plain HTTP page loaded", "neverssl.com" in fp["url"], fp["url"])
            check("camoufox: plain HTTP went through the SSH server", any(f.endswith(":80") for f in new), str(new[:4]))
        show(f"{browser} via ssh", fp, keys=("timezone", "languages", "webrtc"))
        stop(u)
    status, r = add_proxy("S7 ssh wrong password", "nope")
    if status == 200: proxies.append(r["data"]["uuid"])
    status, r = api("POST", "/api/v2/automation/profiles", {"title": "S7 wrong password", "fingerprint": {"os": "win"}, "proxy": {"uuid": proxies[-1]}})
    check("a profile on an SSH proxy with the wrong password is refused", status == 400, f"{status} {str(r)[:140]}")
    if status == 200: created.append(r["data"]["uuid"])
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    if proxies: api("DELETE", "/api/v2/automation/proxies", {"uuids": proxies})
    server.terminate()
    log.close()
summary()
