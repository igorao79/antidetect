"""Which fingerprint surfaces of a CloakBrowser profile are its own, read from two profiles side by side.

On a Pro build (Chromium 148 and later) every image surface has to differ
between profiles and hold across a restart. The free build is reported, not
judged: it is known to leave toDataURL and toBlob as the machine's own.
"""
from scn import *
wait_api()

def surfaces(started):
    out = subprocess.run(["node", "cdp-nav-eval.mjs", str(started["debug_port"]), "https://example.com", "fingerprint-surfaces.js", "5000"], cwd=HERE, capture_output=True, text=True, timeout=120)
    return json.loads(out.stdout) if out.stdout.strip().startswith("{") else {"error": (out.stdout + out.stderr)[-300:]}

IMAGE = ("canvasToDataURL", "canvasToBlob", "canvasGetImageData", "webglReadPixels", "webglToDataURL")
SHARED = ("audioSum", "clientRects", "textWidth")
created = []
try:
    a = create("S13 first", "mac", browser="cloakbrowser"); created.append(a)
    b = create("S13 second", "mac", browser="cloakbrowser"); created.append(b)
    listed = {p["uuid"]: p for p in api("GET", "/api/v2/automation/profiles")[1]["data"]}
    version = listed[a]["version"]
    pro = int(version.split(".")[0]) >= 148
    print(f"CloakBrowser {version} ({'Pro' if pro else 'free'} build)")

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
    check("the browser is the version the profile names", f"Chrome/{version.split('.')[0]}." in (first.get("userAgent") or ""), first.get("userAgent"))
    for key in IMAGE:
        differs, holds = first.get(key) != second.get(key), first.get(key) == again.get(key)
        line = f"{key}: {'its own' if differs else 'SHARED by both profiles'}, {'the same after a restart' if holds else 'CHANGED after a restart'}"
        if pro: check(line, differs and holds, f"{first.get(key)} / {second.get(key)} / {again.get(key)}")
        else: note(line)
    check("drawing the same canvas twice on one page gives one result", first.get("canvasToDataURL") == first.get("canvasToDataURLAgain"))
    check("the two profiles present different graphics hardware", first.get("webglRenderer") != second.get("webglRenderer"), f"{first.get('webglRenderer')} / {second.get('webglRenderer')}")
    for key in SHARED:
        note(f"{key}: {'its own' if first.get(key) != second.get(key) else 'the same in both profiles (the value of this class of machine, not noise)'}")
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
summary()
