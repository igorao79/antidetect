"""Synchronizer on Camoufox: what is done in the leader happens in the follower.

The synchronizer holds the one WebDriver BiDi session each browser takes, so
the leader is driven and both pages are read through it, with the debug-build
route `/api/v2/automation/synchronizer/bidi`. Input sent that way reaches the
page as trusted events, the same as a person's, and goes through the same
capture.
"""
from scn import *
import http.server, threading
wait_api()
PORT = 18791
PAGE = b"""<!doctype html><body style="margin:0;height:5000px">
<button id="b" style="position:absolute;left:40px;top:40px;width:200px;height:60px">click</button>
<input id="i" style="position:absolute;left:40px;top:140px;width:300px;height:40px">
<script>let clicks = 0; b.onclick = () => { clicks++; };
window.state = () => ({ clicks, text: i.value, scrollY: Math.round(scrollY), url: location.href });</script></body>"""

class Site(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.send_header("Content-Type", "text/html"); self.send_header("Content-Length", str(len(PAGE))); self.end_headers(); self.wfile.write(PAGE)
    def log_message(self, *args): pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Site)
threading.Thread(target=server.serve_forever, daemon=True).start()
SYNC = "/api/v2/automation/synchronizer"
URL = f"http://127.0.0.1:{PORT}/"

def bidi(uuid, method, params):
    status, r = api("POST", SYNC + "/bidi", {"uuid": uuid, "method": method, "params": params})
    if status != 200 or not r.get("success"):
        raise RuntimeError(f"{method}: {status} {str(r)[:200]}")
    return r["data"]

def context(uuid):
    return bidi(uuid, "browsingContext.getTree", {"maxDepth": 0})["contexts"][0]["context"]

def evaluate(uuid, expression):
    r = bidi(uuid, "script.evaluate", {"expression": expression, "target": {"context": context(uuid)}, "awaitPromise": False})
    return r.get("result", {}).get("value")

def state(uuid):
    try: return json.loads(evaluate(uuid, "JSON.stringify(typeof window.state === 'function' ? window.state() : { url: location.href })"))
    except (RuntimeError, TypeError, ValueError) as e: return {"error": str(e)[:200]}

def act(uuid, *actions):
    bidi(uuid, "input.performActions", {"context": context(uuid), "actions": list(actions)})

def click(uuid, x, y):
    act(uuid, {"type": "pointer", "id": "scn-mouse", "parameters": {"pointerType": "mouse"}, "actions": [
        {"type": "pointerMove", "x": x, "y": y, "origin": "viewport"}, {"type": "pointerDown", "button": 0}, {"type": "pointerUp", "button": 0}]})
    time.sleep(0.4)

def wait_for(read, done, timeout=20):
    deadline, seen = time.time() + timeout, None
    while time.time() < deadline:
        seen = read()
        if done(seen): break
        time.sleep(1)
    return seen

created, session = [], None
try:
    leader = create("S12 leader", "win"); created.append(leader)
    follower = create("S12 follower", "win"); created.append(follower)

    status, r = api("POST", SYNC, {"leader": leader, "followers": [follower], "headless": HEADLESS})
    check("a session of two Camoufox profiles starts", status == 200 and r.get("success"), f"{status} {str(r)[:200]}")
    session = r["data"]["id"]
    active = [p["uuid"] for p in api("GET", "/api/profiles/active")[1]]
    check("both profiles are running", leader in active and follower in active, str(active)[:120])

    # Like an address-bar navigation: no input in the page before it.
    ctx = context(leader)
    try: bidi(leader, "browsingContext.navigate", {"context": ctx, "url": URL, "wait": "complete"})
    except RuntimeError: bidi(leader, "browsingContext.navigate", {"context": ctx, "url": URL, "wait": "complete"})
    seen = wait_for(lambda: state(follower), lambda s: s.get("url") == URL and "clicks" in s)
    check("the follower went to the same page", seen.get("url") == URL, str(seen)[:160])
    check("the page cannot see the capture", evaluate(leader, "typeof window.__owlSyncInstalled") == "undefined")
    time.sleep(1)

    click(leader, 140, 70); click(leader, 140, 70)
    click(leader, 190, 160)
    for ch in "hi":
        act(leader, {"type": "key", "id": "scn-keys", "actions": [{"type": "keyDown", "value": ch}, {"type": "keyUp", "value": ch}]})
        time.sleep(0.2)
    act(leader, {"type": "wheel", "id": "scn-wheel", "actions": [{"type": "scroll", "x": 500, "y": 300, "deltaX": 0, "deltaY": 400}]})
    time.sleep(1)

    did = state(leader)
    seen = wait_for(lambda: state(follower), lambda s: s.get("text") == did.get("text") and s.get("scrollY", 0) > 0, timeout=10)
    print("    leader:  ", did); print("    follower:", seen)
    check("the leader's own page took the input", did.get("clicks") == 2 and did.get("text") == "hi" and did.get("scrollY", 0) > 0, str(did)[:160])
    check("the follower got the same clicks", seen.get("clicks") == did.get("clicks"), f"{seen.get('clicks')} vs {did.get('clicks')}")
    check("the follower got the same typing", seen.get("text") == did.get("text"), f"{seen.get('text')!r} vs {did.get('text')!r}")
    check("the follower scrolled", seen.get("scrollY", 0) > 0, str(seen.get("scrollY")))

    status, r = api("DELETE", SYNC, {"session_id": session})
    check("the session stops", status == 200, f"{status} {str(r)[:100]}"); session = None
    time.sleep(4)
    still = [p["uuid"] for p in api("GET", "/api/profiles/active")[1] if p["uuid"] in created]
    check("stopping the session closes its profiles", not still, str(still))

    # Session restore only follows a clean quit, which needs the BiDi session
    # the synchronizer held to have been ended first.
    status, st = start(leader)
    tabs = []
    if status == 200:
        for _ in range(15):
            out = subprocess.run(["node", "bidi-tabs.mjs", st["ws_endpoint"]], cwd=HERE, capture_output=True, text=True, timeout=60)
            tabs = json.loads(out.stdout) if out.stdout.strip().startswith("[") else []
            if any(t["url"] == URL for t in tabs): break
            time.sleep(2)
    check("the leader quit cleanly: its page reopens at the next start", any(t["url"] == URL for t in tabs), str(tabs)[:200])
finally:
    if session: api("DELETE", SYNC, {"session_id": session})
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    server.shutdown()
summary()
