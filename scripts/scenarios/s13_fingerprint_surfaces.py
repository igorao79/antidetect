"""Which fingerprint surfaces of a Camoufox profile are its own, read from two profiles side by side.

Measured on Camoufox 156: toDataURL and toBlob carry Firefox's own per-session
canvas randomization (EfficientCanvasRandomization, as in stock Firefox), so
two profiles never share them and one profile changes at every launch.
getImageData, WebGL readPixels and text metrics are this machine's, the same
in every profile: canvas:aaOffset and canvas:seed do not reach them in this
build. The audio fingerprint follows audio:seed, its own and stable.
"""
from scn import *
wait_api()

def surfaces(started):
    out = subprocess.run(["node", "bidi-eval.mjs", started["ws_endpoint"], "https://example.com", "fingerprint-surfaces.js", "5000"], cwd=HERE, capture_output=True, text=True, timeout=120)
    return json.loads(out.stdout) if out.stdout.strip().startswith("{") else {"error": (out.stdout + out.stderr)[-300:]}

RANDOMIZED = ("canvasToDataURL", "canvasToBlob", "webglToDataURL")
MACHINE = ("canvasGetImageData", "webglReadPixels", "clientRects", "textWidth")
created = []
try:
    a = create("S13 first", "win"); created.append(a)
    b = create("S13 second", "win"); created.append(b)
    listed = {p["uuid"]: p for p in api("GET", "/api/v2/automation/profiles")[1]["data"]}
    print(f"Camoufox {listed[a]['version']}")

    read = {}
    for uuid in (a, b):
        status, st = start(uuid)
        check(f"{listed[uuid]['title']}: starts", status == 200, f"{status} {str(st)[:200]}")
        if status != 200: continue
        time.sleep(4)
        read[uuid] = surfaces(st)
        stop(uuid)
    time.sleep(3)
    status, st = start(a); time.sleep(4)
    again = surfaces(st) if status == 200 else {"error": str(st)}
    stop(a)

    first, second = read.get(a, {}), read.get(b, {})
    check("both profiles were read", "error" not in first and "error" not in second and "error" not in again, str(first.get("error") or second.get("error") or again.get("error"))[:200])
    print(f"    {first.get('userAgent')}")
    check("drawing the same canvas twice on one page gives one result", first.get("canvasToDataURL") == first.get("canvasToDataURLAgain"))
    for key in RANDOMIZED:
        check(f"{key}: not shared by the two profiles", first.get(key) != second.get(key), f"{first.get(key)} / {second.get(key)}")
        note(f"{key}: {'the same' if first.get(key) == again.get(key) else 'changed'} after a restart (stock Firefox changes it per session)")
    check("audio: each profile's own", first.get("audioSamples") != second.get("audioSamples"), f"{first.get('audioSamples')} / {second.get('audioSamples')}")
    check("audio: the same after a restart", first.get("audioSamples") == again.get("audioSamples"), f"{first.get('audioSamples')} / {again.get('audioSamples')}")
    for key in MACHINE:
        note(f"{key}: {'its own' if first.get(key) != second.get(key) else 'the same in both profiles (this machine, not noise)'}")
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
summary()
