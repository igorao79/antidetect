from scn import *
import scn
wait_api()
# Uses a profile s1 leaves running.
ids = json.load(open("s1-ids.json")); running = ids["mac"]
status, r = start(running)
check("starting a running profile is a 409", status == 409, f"{status} {r}")
status, r = start("00000000000000000000000000000000")
check("unknown uuid is a 404", status == 404, f"{status} {r}")
status, r = api("POST", "/api/profiles/start", {"headless": True})
check("missing uuid is rejected (4xx)", 400 <= status < 500, f"{status}")
status, r = api("POST", "/api/profiles/stop", {"uuid": "00000000000000000000000000000000"})
check("stopping an unknown profile is a 404", status == 404, f"{status}")
# dashed uuid form addresses the same profile
dashed = f"{running[:8]}-{running[8:12]}-{running[12:16]}-{running[16:20]}-{running[20:]}"
status, r = start(dashed)
check("dashed uuid resolves to the same profile (409: running)", status == 409, f"{status}")
status, text = api("POST", "/api/profiles/start", {"uuid": running}, headers={"Host": "evil.example.com"}, raw=True)
check("foreign Host header is refused (DNS rebinding)", status == 403, f"{status} {text[:60]}")
req = urllib.request.Request(BASE + "/api/profiles/stop", data=json.dumps({"uuid": running}).encode(), method="POST", headers={"Content-Type": "text/plain"})
try:
    with scn._opener.open(req, timeout=20) as resp: st = resp.status
except urllib.error.HTTPError as e: st = e.code
check("non-JSON content type is refused (cross-site form post)", st == 415, str(st))
status, active = api("GET", "/api/profiles/active")
check("the text/plain stop did not stop the profile", running in [p["uuid"] for p in active])
status, r = api("POST", "/api/v2/automation/profiles", {"title": "", "fingerprint": {"os": "win"}})
check("empty title is rejected", status == 400, f"{status} {str(r)[:80]}")
status, r = api("POST", "/api/v2/automation/profiles", {"title": "x", "fingerprint": {"os": "android"}})
check("unsupported fingerprint os is rejected", status == 400, f"{status} {str(r)[:80]}")
status, r = api("POST", "/api/v2/automation/profiles", {"title": "x", "browser": "netscape"})
check("unsupported browser is rejected", status == 400, f"{status} {str(r)[:80]}")
# duplicate names
u1 = create("S4 dup", "win")
status, r = api("POST", "/api/v2/automation/profiles", {"title": "S4 dup", "fingerprint": {"os": "win"}})
check("duplicate title is handled (rejected or accepted, not a 500)", status in (200, 400), f"{status} {str(r)[:100]}")
extra = [u1] + ([r["data"]["uuid"]] if status == 200 else [])
status, r = api("POST", "/api/profiles/start", {"uuid": u1, "debug_port": 80})
check("privileged debug_port is rejected", status == 400, f"{status} {r}")
# headless start, tags, list, delete
status, st = start(u1, headless=True)
check("headless start", status == 200 and st.get("headless") is True, str(st)[:120])
time.sleep(4)
fp = probe(st, url="https://example.com")
check("headless profile still presents its fingerprint", fp["platform"] == "Win32" and fp["webdriver"] is False, fp["platform"])
status, r = delete(u1)
check("deleting a running profile is refused or stops it cleanly (no 500)", status in (200, 400, 409), f"{status} {str(r)[:120]}")
stop(u1); time.sleep(2)
status, r = delete(*extra)
check("delete after stop", status == 200, f"{status} {str(r)[:100]}")
status, lst = api("GET", "/api/v2/automation/profiles")
check("deleted profiles leave the list", not any(p["uuid"] in extra for p in lst["data"]))
summary()
