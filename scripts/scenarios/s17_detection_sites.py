"""Public fingerprint checkers through a real remote exit, the way a customer would test.

Opens one Windows and one macOS Camoufox profile and reads what
bot.sannysoft.com, browserscan.net, CreepJS and browserleaks' WebRTC page
report; the text each site prints goes to stdout for the report (`SCN_DEBUG=1`
prints more of it). With SCN_REMOTE_PROXY=http://user:password@host:port the
profiles go through that proxy and every page must show its address and none
of this machine's; without it they connect directly and the address checks
are left out.
"""
from scn import *
from urllib.parse import urlparse, unquote
import re
REMOTE = os.environ.get("SCN_REMOTE_PROXY", "").strip()
if not REMOTE:
    print("NOTE no SCN_REMOTE_PROXY: profiles connect directly, address checks are left out")
wait_api()
remote = urlparse(REMOTE)
TEXT = os.path.join(HERE, "page-text.js")
with open(TEXT, "w") as f:
    f.write("document.body ? document.body.innerText : ''")

def via(url):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({"http": REMOTE, "https": REMOTE}))
    with opener.open(url, timeout=30) as r:
        return r.read().decode().strip()

host_ip = public_ip()
exit_ip = via("https://api.ipify.org") if REMOTE else host_ip
if REMOTE: check("the remote proxy exits somewhere else than this machine", exit_ip != host_ip)

def page(started, url, wait, script=TEXT):
    out = subprocess.run(["node", "bidi-eval.mjs", started["ws_endpoint"], url, script, str(wait)], cwd=HERE, capture_output=True, text=True, timeout=240)
    return out.stdout.strip() or out.stderr.strip()

def around(text, pattern, width=90):
    m = re.search(pattern, text, re.I)
    return text[max(0, m.start() - 10): m.end() + width].replace("\n", " | ") if m else None

DEBUG = os.environ.get("SCN_DEBUG") == "1"
created, proxies = [], []
try:
    if REMOTE:
        status, r = api("POST", "/api/v2/automation/proxies", {"title": f"S17 remote {int(time.time())}", "type": "http", "host": remote.hostname, "port": remote.port, "login": unquote(remote.username or ""), "password": unquote(remote.password or "")})
        assert status == 200, (status, r)
        proxies.append(r["data"]["uuid"])
    for os_name in ("win", "mac"):
        u = create(f"S17 {os_name}", os_name, proxy=proxies[0] if proxies else None); created.append(u)
        status, st = start(u)
        check(f"{os_name}: start", status == 200, f"{status} {str(st)[:160]}")
        if status != 200: continue
        time.sleep(4)

        raw = page(st, "https://bot.sannysoft.com/", 9000, script="sannysoft.js")
        try:
            s = json.loads(raw)
            failed = [row["name"] for row in s["failed"] if not row["name"].startswith("Chrome")]
            print(f"--- {os_name} sannysoft: {s['passed']} passed, failed {failed or 'none'}, warn {[w['name'] for w in s['warn']] or 'none'}")
            check(f"{os_name}: sannysoft reports no failed test", not failed and s["passed"] >= 10)
        except ValueError:
            check(f"{os_name}: sannysoft page read", False, raw[:200])

        text = page(st, "https://www.browserscan.net/bot-detection", 15000)
        verdict = around(text, r"Test Results?", 60)
        print(f"--- {os_name} browserscan bot detection: {verdict}")
        if DEBUG: print(text[:3000])
        check(f"{os_name}: browserscan does not call it a bot", verdict is not None and not re.search(r"\bRobot\b|Bot detected", verdict, re.I), verdict or text[:200])

        text = page(st, "https://www.browserscan.net/", 20000)
        for label in (r"Browser fingerprint authenticity", r"authenticity", r"IP Address", r"Time ?zone", r"WebRTC"):
            snippet = around(text, label, 80)
            if snippet: print(f"    browserscan: {snippet}")
        if DEBUG: print(text[:4000])
        if REMOTE:
            check(f"{os_name}: browserscan never shows this machine's address", host_ip not in text)
            check(f"{os_name}: browserscan shows the proxy's address", exit_ip in text)

        text = page(st, "https://abrahamjuliot.github.io/creepjs/", 25000)
        for label in (r"\d+% like headless", r"\d+% headless", r"\d+% stealth", r"lies \(\d+\)", r"trust score"):
            snippet = around(text, label, 40)
            if snippet: print(f"    creepjs: {snippet}")
        if DEBUG: print(text[:4000])
        lies = re.search(r"lies \((\d+)\)", text, re.I)
        check(f"{os_name}: CreepJS page read", len(text) > 500, text[:200])
        if lies: check(f"{os_name}: CreepJS finds no lies", lies.group(1) == "0", lies.group(0))

        text = page(st, "https://browserleaks.com/webrtc", 12000)
        if DEBUG: print(text[:3000])
        if REMOTE: check(f"{os_name}: browserleaks WebRTC never shows this machine's address", host_ip not in text)
        print(f"    browserleaks webrtc: {around(text, 'Public IP', 80) or around(text, 'WebRTC Leak', 80)}")
        stop(u)
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    if proxies: api("DELETE", "/api/v2/automation/proxies", {"uuids": proxies})
    try: os.remove(TEXT)
    except OSError: pass
summary()
