"""Synchronizer on CloakBrowser: what is done in the leader happens in the follower."""
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

def drive(*args):
    out = subprocess.run(["node", "sync-drive.mjs", *map(str, args)], cwd=HERE, capture_output=True, text=True, timeout=120)
    try: return json.loads(out.stdout)
    except ValueError: return {"error": (out.stdout + out.stderr)[-300:]}

created, session = [], None
try:
    leader = create("S12 leader", "win", browser="cloakbrowser"); created.append(leader)
    follower = create("S12 follower", "win", browser="cloakbrowser"); created.append(follower)
    firefox = create("S12 camoufox", "win", browser="camoufox"); created.append(firefox)

    status, r = api("POST", SYNC, {"leader": leader, "followers": [firefox], "headless": True})
    check("a Camoufox profile is refused as a follower", status == 400, f"{status} {str(r)[:140]}")

    status, r = api("POST", SYNC, {"leader": leader, "followers": [follower], "headless": True})
    check("a session of two CloakBrowser profiles starts", status == 200 and r.get("success"), f"{status} {str(r)[:200]}")
    session = r["data"]["id"]
    ports = {p["uuid"]: p["debug_port"] for p in api("GET", "/api/profiles/active")[1]}
    check("both profiles are running", leader in ports and follower in ports, str(ports)[:120])

    did = drive("act", ports[leader], f"http://127.0.0.1:{PORT}/")
    time.sleep(2)
    seen = drive("read", ports[follower])
    print("    leader:  ", did); print("    follower:", seen)
    check("the leader's own page took the input", did.get("clicks") == 2 and did.get("text") == "hi" and did.get("scrollY", 0) > 0, str(did)[:160])
    check("the follower went to the same page", seen.get("url") == did.get("url"), str(seen.get("url")))
    check("the follower got the same clicks", seen.get("clicks") == did.get("clicks"), f"{seen.get('clicks')} vs {did.get('clicks')}")
    check("the follower got the same typing", seen.get("text") == did.get("text"), f"{seen.get('text')!r} vs {did.get('text')!r}")
    check("the follower scrolled", seen.get("scrollY", 0) > 0, str(seen.get("scrollY")))

    status, r = api("DELETE", SYNC, {"session_id": session})
    check("the session stops", status == 200, f"{status} {str(r)[:100]}"); session = None
    time.sleep(4)
    still = [p["uuid"] for p in api("GET", "/api/profiles/active")[1] if p["uuid"] in created]
    check("stopping the session closes its profiles", not still, str(still))
finally:
    if session: api("DELETE", SYNC, {"session_id": session})
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    server.shutdown()
summary()
