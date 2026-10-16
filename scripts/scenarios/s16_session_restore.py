"""Session restore: a Camoufox profile reopens the tabs it had, with their session storage.

A stop asks the browser to quit over BiDi; the launch then resumes that
session the way Firefox resumes a crashed one, without the restore prompt.
"""
from scn import *
wait_api()

MARK = "owl-restore"
def tabs(started, mark=False):
    args = ["node", "bidi-tabs.mjs", started["ws_endpoint"]] + (["--mark", MARK] if mark else [])
    out = subprocess.run(args, cwd=HERE, capture_output=True, text=True, timeout=60)
    return json.loads(out.stdout) if out.stdout.strip().startswith("[") else []

def wait_tabs(started, done, mark=False, timeout=45):
    """The open tabs once `done(tabs)` holds, or the last reading at the timeout."""
    deadline, seen = time.time() + timeout, []
    while time.time() < deadline:
        seen = tabs(started, mark)
        if os.environ.get("SCN_DEBUG"): print("   ", seen)
        if done(seen): break
        time.sleep(2)
    return seen

created = []
try:
    u = create("S16 restore", "win"); created.append(u)
    status, st = start(u, url=f"https://example.com/?{MARK}")
    check("the profile starts on a page", status == 200, f"{status} {str(st)[:200]}")
    before = wait_tabs(st, lambda ts: any(MARK in t["url"] and t["owl"] == "1" for t in ts), mark=True)
    check("the page is open and marked", any(MARK in t["url"] and t["owl"] == "1" for t in before), str(before))
    time.sleep(7)  # past browser.sessionstore.interval
    stop(u); time.sleep(3)

    status, st = start(u)
    check("the profile starts again without a page", status == 200, f"{status} {str(st)[:200]}")
    # Session storage is put back a moment after the restored page loads.
    after = wait_tabs(st, lambda ts: any(MARK in t["url"] and t["owl"] == "1" for t in ts))
    restored = [t for t in after if MARK in t["url"]]
    check("the tab is open again", bool(restored), str(after))
    check("its session storage came back with it", any(t["owl"] == "1" for t in restored), str(restored))
    check("no restore prompt in its place", not any(t["url"].startswith("about:sessionrestore") for t in after), str(after))
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
summary()
