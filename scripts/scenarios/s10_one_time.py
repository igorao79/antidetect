"""One-time profiles: created and started in one call, gone once their browser stops."""
from scn import *
import signal
wait_api()

def exists(uuid):
    # `stop` answers 404 for a profile that is no longer there.
    return api("POST", "/api/profiles/stop", {"uuid": uuid})[0] != 404

def wait_gone(uuid, timeout=45):
    deadline = time.time() + timeout
    while time.time() < deadline:
        active = api("GET", "/api/profiles/active")[1]
        if not any(p["uuid"] == uuid for p in active) and not exists(uuid): return True
        time.sleep(3)
    return False

# A dev build keeps its data in AntidetectDev, a release build in Antidetect.
PROFILE_DIRS = [os.path.expanduser(f"~/Library/Application Support/{app}/profiles") for app in ("AntidetectDev", "Antidetect")]

def on_disk(uuid):
    dashed = "-".join((uuid[:8], uuid[8:12], uuid[12:16], uuid[16:20], uuid[20:]))
    return any(os.path.isdir(os.path.join(root, dashed)) for root in PROFILE_DIRS)

def wait_gone_untouched(uuid, timeout=90):
    """Like wait_gone, but without asking the app to stop anything."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        active = api("GET", "/api/profiles/active")[1]
        if not any(p["uuid"] == uuid for p in active) and not on_disk(uuid): return True
        time.sleep(3)
    return False

leftovers = []
try:
    for browser in ("camoufox",):
        status, st = api("POST", "/api/profiles/one_time/start", {"browser": browser, "fingerprint": {"os": "win"}, "headless": True})
        check(f"{browser}: a one-time profile starts", status == 200 and st.get("state") == "STARTED", f"{status} {str(st)[:160]}")
        if status != 200: continue
        uuid = st["uuid"]; leftovers.append(uuid)
        time.sleep(4)
        fp = probe(st, url="https://example.com")
        check(f"{browser}: it browses like any profile", fp["url"].startswith("https://example.com") and fp["webdriver"] is False, fp["url"])
        listed = api("GET", "/api/v2/automation/profiles")[1]["data"]
        check(f"{browser}: it is not in the profile list", not any(p["uuid"] == uuid for p in listed))
        status, _ = stop(uuid)
        check(f"{browser}: it stops", status == 200, str(status))
        check(f"{browser}: it is deleted once stopped", wait_gone(uuid))

        # The same when its window is closed instead of the API stopping it.
        status, st = api("POST", "/api/profiles/one_time/start", {"browser": browser, "headless": True})
        if status != 200: check(f"{browser}: a second one-time profile starts", False, str(st)[:160]); continue
        uuid = st["uuid"]; leftovers.append(uuid)
        time.sleep(5)
        check(f"{browser}: it exists on disk while it runs", on_disk(uuid))
        os.kill(st["browser_pid"], signal.SIGTERM)
        check(f"{browser}: it is deleted after its browser exits on its own", wait_gone_untouched(uuid))

    status, r = api("POST", "/api/profiles/one_time/start", {"browser": "netscape"})
    check("an unknown engine is refused", status == 400, f"{status} {str(r)[:100]}")
finally:
    for u in leftovers: stop(u)
summary()
