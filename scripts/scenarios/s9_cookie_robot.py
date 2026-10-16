"""Cookie Robot: profiles are opened, walked through a site list and closed, and keep the cookies they were given."""
from scn import *
import http.server, threading
wait_api()
PORT = 18790
hits = []

class Site(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        hits.append(self.path)
        body = b"<html><body style='height:6000px'>cookie robot test page</body></html>"
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.send_header("Content-Length", str(len(body)))
        # /check only reads: a cookie seen there was set by an earlier visit.
        if self.path != "/check":
            self.send_header("Set-Cookie", "robot=1; Max-Age=86400; Path=/")
        self.end_headers(); self.wfile.write(body)
    def log_message(self, *args): pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Site)
threading.Thread(target=server.serve_forever, daemon=True).start()
ROBOT = "/api/v2/automation/cookie_robot"
SITE = f"http://127.0.0.1:{PORT}"

def wait_finished(timeout=240):
    deadline = time.time() + timeout
    while time.time() < deadline:
        status, r = api("GET", ROBOT)
        if status == 200 and not r["data"]["running"]: return r["data"]
        time.sleep(2)
    raise RuntimeError("the robot did not finish")

created = []

def still_open():
    """Which of this scenario's profiles are running (others may be, from s1)."""
    return [p["uuid"] for p in api("GET", "/api/profiles/active")[1] if p["uuid"] in created]

try:
    for browser in ("camoufox",):
        created.append(create(f"S9 {browser}", "win", browser=browser))

    status, r = api("POST", ROBOT, {"uuids": created, "urls": ["ftp://files.example"]})
    check("a site that is not a web address is refused", status == 400 and "COOKIE_ROBOT_INVALID_URL" in str(r), f"{status} {str(r)[:120]}")
    status, r = api("POST", ROBOT, {"uuids": created, "urls": ["  "]})
    check("a run without sites is refused", status == 400 and "COOKIE_ROBOT_NO_SITES" in str(r), f"{status} {str(r)[:120]}")

    sites = [f"{SITE}/first", f"{SITE}/second", "https://no-such-host.invalid/"]
    status, r = api("POST", ROBOT, {"uuids": created, "urls": sites, "min_seconds": 3, "max_seconds": 4, "headless": True, "parallel": 2})
    check("the robot starts", status == 200 and r["data"]["running"], f"{status} {str(r)[:160]}")
    status, r = api("POST", ROBOT, {"uuids": created, "urls": sites})
    check("a second run is refused while one is going", status == 409, f"{status} {str(r)[:120]}")

    done = wait_finished()
    for p in done["profiles"]:
        check(f"{p['name']}: finished", p["state"] == "done", f"{p['state']} {p.get('error')}")
        check(f"{p['name']}: two sites opened, one did not", (p["visited"], p["unreachable"]) == (2, 1), f"visited {p['visited']}, unreachable {p['unreachable']}")
    check("each profile reached both pages", sum("/first" in h for h in hits) >= 2 and sum("/second" in h for h in hits) >= 2, f"{len(hits)} requests")
    check("the robot closed the profiles it opened", not still_open(), str(still_open()))

    for u in created:
        status, st = start(u)
        if status != 200: check("profile starts after the robot", False, str(st)[:160]); continue
        time.sleep(4)
        fp = probe(st, url=f"{SITE}/check")
        check(f"{st['browser']}: the cookie from the robot's visit is still there", "robot=1" in (fp.get("cookie") or ""), fp.get("cookie"))
        stop(u)
    time.sleep(3)

    status, r = api("POST", ROBOT, {"uuids": created, "urls": [f"{SITE}/slow"], "min_seconds": 120, "max_seconds": 120, "headless": True, "parallel": 2})
    check("a long run starts", status == 200, f"{status} {str(r)[:120]}")
    time.sleep(20)
    status, r = api("DELETE", ROBOT)
    check("the run can be stopped", status == 200 and r["data"]["stopped"], str(r)[:120])
    stopped = wait_finished(90)
    check("stopped profiles are reported as stopped", all(p["state"] == "cancelled" for p in stopped["profiles"]), str([p["state"] for p in stopped["profiles"]]))
    check("nothing of the run is left open after a stop", not still_open(), str(still_open()))
finally:
    api("DELETE", ROBOT)
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    server.shutdown()
summary()
