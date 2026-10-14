"""Screen: a CloakBrowser profile reports a stable screen of its own instead of this machine's.

Left alone, a Pro build reports the real display and the space the real Dock
leaves, so profiles share a screen, a Windows persona shows a MacBook panel,
and the available height changes when the Dock does. The launch now gives
every profile a size of its persona, drawn from its seed.
"""
from scn import *
wait_api()
MAC = {"1440x900", "1470x956", "1512x982", "1680x1050", "1728x1117", "1792x1120", "1800x1169", "1920x1080", "2056x1329", "2560x1440"}
DESKTOP = {"1536x864", "1600x900", "1680x1050", "1920x1080", "1920x1200", "2560x1080", "2560x1440", "3440x1440"}

def read(uuid, label):
    status, st = start(uuid)
    check(f"{label}: starts", status == 200, f"{status} {str(st)[:160]}")
    if status != 200: return None, False
    time.sleep(4)
    fp = probe(st, url="https://example.com")
    show(label, fp, keys=("screen", "avail", "screenInMedia", "platform"))
    version = api("GET", "/api/v2/automation/profiles/" + uuid)[1]["data"]["version"]
    stop(uuid); time.sleep(3)
    return fp, not version.startswith("145.")

created = []
try:
    seen = []
    for index in range(1, 4):
        u = create(f"S15 mac {index}", "mac", browser="cloakbrowser"); created.append(u)
        fp, pro = read(u, f"macOS profile {index}")
        if not fp: continue
        seen.append(fp["screen"])
        if pro:
            check(f"macOS profile {index}: a screen a Mac has", fp["screen"] in MAC, fp["screen"])
            check(f"macOS profile {index}: CSS media queries report the same screen", fp["screenInMedia"] is True, str(fp["screenInMedia"]))
        if index == 1:
            again, _ = read(u, "macOS profile 1, second launch")
            if again:
                check("the same screen and available area at the next launch", (again["screen"], again["avail"]) == (fp["screen"], fp["avail"]), f"{fp['screen']} {fp['avail']} then {again['screen']} {again['avail']}")
    if seen and pro:
        note(f"screens drawn: {seen}")
    u = create("S15 windows", "win", browser="cloakbrowser"); created.append(u)
    fp, pro = read(u, "Windows profile")
    if fp and pro:
        check("Windows profile: a screen a desktop has, not this Mac's panel", fp["screen"] in DESKTOP, fp["screen"])
        check("Windows profile: CSS media queries report the same screen", fp["screenInMedia"] is True, str(fp["screenInMedia"]))
    if fp and not pro:
        note("free build: the screen stays the real display, the only size its media queries agree with")
        check("free build: the screen agrees with CSS media queries", fp["screenInMedia"] is True, f"{fp['screen']} {fp['screenInMedia']}")
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
summary()
