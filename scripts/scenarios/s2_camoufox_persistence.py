from scn import *
# Uses the profiles s1 leaves running.
ids = json.load(open("s1-ids.json"))
uuid = ids["win"]
# Wait for the app (it rebuilds after a source change).
wait_api()
status, active = api("GET", "/api/profiles/active")
running = {p["uuid"]: p for p in active}
cam = [p for p in running.values() if p["browser"] == "camoufox"]
check("running profiles report a debug port in /active", len(cam) == 3 and all(p["debug_port"] for p in cam), str([p["debug_port"] for p in cam]))
check("recovered ws_endpoint is usable", running[uuid]["ws_endpoint"].endswith("/session"), running[uuid]["ws_endpoint"])
stop(uuid); time.sleep(3)

status, started = start(uuid)
# The first emoji drawn in a session uses a fallback glyph and makes Firefox
# load the emoji font in the background; every later drawing uses that font.
# Shapes and Latin text do not change. So the first reading is thrown away:
# it differs once, and not because of canvas noise.
check("start", status == 200, str(started)[:160]); time.sleep(4)
probe(started, url="https://example.com"); time.sleep(6)
before = probe(started, url="https://example.com", set_cookie="antidetect_session=abc123")
again = probe(started, url="https://example.com")
check("canvas stable within a session", before["canvasHash"] == again["canvasHash"])
# Camoufox 156 keeps stock Firefox's per-session canvas randomization.
status, r = stop(uuid)
check("stop returns 200", status == 200, str(r)); time.sleep(3)
status, active = api("GET", "/api/profiles/active")
check("stopped profile leaves the active list", uuid not in [p["uuid"] for p in active])

status, restarted = start(uuid)
check("restart", status == 200, str(restarted)[:160]); time.sleep(4)
after = probe(restarted, url="https://example.com")
show("after restart", after)
for key in ("userAgent", "platform", "screen", "cores", "webglRenderer", "timezone", "languages"):
    check(f"same {key} after restart", before[key] == after[key], f"{before[key]} -> {after[key]}")
check("canvas is re-randomized per session (stock Firefox behaviour)", before["canvasHash"] != after["canvasHash"], f'{before["canvasHash"]} -> {after["canvasHash"]}')
check("cookie survived the restart", "antidetect_session=abc123" in after["cookie"], after["cookie"])
check("a new debug port is issued per start", started["debug_port"] != restarted["debug_port"])
other = probe(running[ids["mac"]], url="https://example.com")
check("another profile has a different canvas", other["canvasHash"] != after["canvasHash"], f'{other["canvasHash"]} vs {after["canvasHash"]}')
summary()
