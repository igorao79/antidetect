"""A real remote exit: the browser shows the proxy's address and timezone and none of this machine's.

Needs SCN_REMOTE_PROXY=http://user:password@host:port, an HTTP proxy whose exit
is somewhere else than this machine. The second half moves a profile's exit
after it was created and expects the next launch to follow it.
"""
from scn import *
from urllib.parse import urlparse, unquote
REMOTE = os.environ.get("SCN_REMOTE_PROXY", "").strip()
if not REMOTE:
    print("SKIP set SCN_REMOTE_PROXY=http://user:password@host:port to run this scenario"); sys.exit(0)
wait_api()
remote = urlparse(REMOTE)
CHAIN_PORT, LOG = 18766, os.path.join(HERE, "chain-proxy.log")

def via(proxy_url, url):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({"http": proxy_url, "https": proxy_url}))
    with opener.open(url, timeout=30) as r:
        return r.read().decode().strip()

def timezone_of(ip):
    try:
        with _opener.open(f"http://ip-api.com/json/{ip}?fields=timezone", timeout=20) as r:
            return json.loads(r.read().decode()).get("timezone")
    except Exception:
        return None

import scn
_opener = scn._opener
host_ip, exit_ip = public_ip(), via(REMOTE, "https://api.ipify.org")
host_tz, exit_tz = timezone_of(host_ip), timezone_of(exit_ip)
print(f"this machine: {host_ip} ({host_tz}); remote exit: {exit_ip} ({exit_tz})")
check("the remote proxy exits somewhere else than this machine", exit_ip != host_ip)

def looks_remote(label, started):
    fp = probe(started, url="https://api.ipify.org")
    show(label, fp, keys=("text", "timezone", "languages", "webrtc"))
    check(f"{label}: sites see the proxy's address", fp["text"] == exit_ip, fp["text"])
    if exit_tz: check(f"{label}: timezone is the exit's", same_tz(fp["timezone"], exit_tz), fp["timezone"])
    check(f"{label}: timezone is not this machine's", not same_tz(fp["timezone"], host_tz) or same_tz(host_tz, exit_tz), fp["timezone"])
    check(f"{label}: WebRTC does not show this machine's address", not any(host_ip in c for c in fp["webrtc"]), str(fp["webrtc"]))
    return fp

created, proxies, chain = [], [], None
def add_proxy(title, host, port, login=None, password=None):
    body = {"title": f"{title} {int(time.time())}", "type": "http", "host": host, "port": port}
    if login: body.update(login=login, password=password)
    status, r = api("POST", "/api/v2/automation/proxies", body)
    assert status == 200, (status, r)
    proxies.append(r["data"]["uuid"]); return r["data"]["uuid"]

def run_chain(upstream=None):
    global chain
    if chain: chain.terminate(); chain.wait(5); time.sleep(1)
    chain = subprocess.Popen(["node", "test-proxy.mjs", str(CHAIN_PORT), LOG] + ([upstream] if upstream else []), cwd=HERE)
    time.sleep(1.5)

try:
    direct = add_proxy("S8 remote", remote.hostname, remote.port, unquote(remote.username or ""), unquote(remote.password or ""))
    for browser in ("camoufox", "cloakbrowser"):
        u = create(f"S8 {browser} remote", "win", browser=browser, proxy=direct); created.append(u)
        status, st = start(u)
        check(f"{browser}: start on the remote proxy", status == 200, f"{status} {str(st)[:160]}")
        if status != 200: continue
        time.sleep(4)
        first = looks_remote(f"{browser} on a remote proxy", st)
        stop(u); time.sleep(3)
        # The language is drawn from the exit country's speakers; a profile
        # has to draw the same one every time.
        status, st = start(u)
        if status != 200: check(f"{browser}: second start on the remote proxy", False, str(st)[:160]); continue
        time.sleep(4)
        again = probe(st, url="https://example.com")
        check(f"{browser}: same language at the next launch", again["languages"] == first["languages"], f"{first['languages']} then {again['languages']}")
        stop(u)

    # The same local endpoint, first leading straight out and then through the
    # remote proxy: the profile's settings never change, its exit does.
    run_chain()
    moving = add_proxy("S8 moving exit", "127.0.0.1", CHAIN_PORT)
    movers = {}
    for browser in ("camoufox", "cloakbrowser"):
        u = create(f"S8 {browser} moving exit", "win", browser=browser, proxy=moving); created.append(u); movers[browser] = u
        status, st = start(u)
        if status != 200: check(f"{browser}: start before the exit moves", False, str(st)[:160]); continue
        time.sleep(4)
        fp = probe(st, url="https://api.ipify.org")
        check(f"{browser}: before the move sites see this machine", fp["text"] == host_ip, fp["text"])
        if host_tz: check(f"{browser}: before the move the timezone is this machine's", same_tz(fp["timezone"], host_tz), fp["timezone"])
        stop(u)
    time.sleep(3)
    run_chain(REMOTE)
    for browser, u in movers.items():
        status, st = start(u)
        check(f"{browser}: start after the exit moved", status == 200, f"{status} {str(st)[:160]}")
        if status != 200: continue
        time.sleep(4)
        looks_remote(f"{browser} after its exit moved", st)
        stop(u)
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    if proxies: api("DELETE", "/api/v2/automation/proxies", {"uuids": proxies})
    if chain: chain.terminate()
summary()
