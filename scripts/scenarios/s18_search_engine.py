"""Address-bar search: a Camoufox profile searches with Google, not the loopback address.

The build's own policy offers only an engine that sends searches to
127.0.0.1; Owl rewrites the policy before every launch.
"""
from scn import *
wait_api()

def engine(started):
    out = subprocess.run(["node", "search-engine.mjs", started["ws_endpoint"]],
                         cwd=HERE, capture_output=True, text=True, timeout=60)
    try: return json.loads(out.stdout)
    except ValueError: return {"error": out.stdout + out.stderr}

created = []
try:
    u = create("S18 search", "win"); created.append(u)
    status, st = start(u)
    check("the profile starts", status == 200, f"{status} {str(st)[:200]}")
    found = engine(st)
    check("Google is the default engine", found.get("name") == "Google", str(found))
    check("a search goes to google.com", found.get("url", "").startswith("https://www.google.com/search?q=owl"), str(found))
    check("the loopback engine is not listed", "None" not in found.get("engines", ["None"]), str(found))
    check("other engines are offered", {"Bing", "DuckDuckGo"} <= set(found.get("engines", [])), str(found))
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
summary()
