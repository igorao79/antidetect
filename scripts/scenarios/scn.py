"""Helpers for driving Antidetect through its Octo-compatible API in test scenarios.

Start the dev app first (without HTTP(S)_PROXY in its environment), then run a
scenario with `python3 <scenario>.py` from this directory.
"""
import json, subprocess, sys, time, urllib.request, urllib.error, os

BASE = "http://127.0.0.1:58888"
HERE = os.path.dirname(os.path.abspath(__file__))
_opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

def api(method, path, body=None, headers=None, raw=False):
    data = json.dumps(body).encode() if body is not None else None
    h = {"Content-Type": "application/json"}
    h.update(headers or {})
    req = urllib.request.Request(BASE + path, data=data, method=method, headers=h)
    try:
        with _opener.open(req, timeout=180) as r:
            status, text = r.status, r.read().decode()
    except urllib.error.HTTPError as e:
        status, text = e.code, e.read().decode()
    if raw:
        return status, text
    try:
        return status, json.loads(text)
    except ValueError:
        return status, text

def create(title, os_name="win", browser="camoufox", proxy=None, tags=None, interface_language=None):
    body = {"title": title, "fingerprint": {"os": os_name}, "browser": browser}
    if proxy: body["proxy"] = {"uuid": proxy}
    if tags: body["tags"] = tags
    if interface_language: body["interface_language"] = interface_language
    status, r = api("POST", "/api/v2/automation/profiles", body)
    assert status == 200 and r.get("success"), (status, r)
    return r["data"]["uuid"]

# Browsers start without a window so a run does not take over the screen;
# SCN_HEADED=1 shows them.
HEADLESS = os.environ.get("SCN_HEADED", "") != "1"

def start(uuid, headless=None, url=None, debug_port=True):
    body = {"uuid": uuid, "headless": HEADLESS if headless is None else headless, "debug_port": debug_port}
    if url: body["url"] = url
    return api("POST", "/api/profiles/start", body)

def stop(uuid):
    return api("POST", "/api/profiles/stop", {"uuid": uuid})

def delete(*uuids):
    return api("DELETE", "/api/v2/automation/profiles", {"uuids": list(uuids)})

def probe(started, url=None, set_cookie=None, tries=4):
    """Read the device fingerprint out of a started profile."""
    args = []
    if started["browser"] == "camoufox":
        args = ["node", "bidi-probe.mjs", started["ws_endpoint"]]
    else:
        args = ["node", "cdp-probe.mjs", str(started["debug_port"])]
    args.append(url or "")
    if set_cookie: args += ["--set-cookie", set_cookie]
    last = ""
    for attempt in range(tries):
        p = subprocess.run(args, cwd=HERE, capture_output=True, text=True, timeout=120)
        if p.returncode == 0 and p.stdout.strip().startswith("{"):
            return json.loads(p.stdout)
        last = (p.stdout + p.stderr).strip()[-300:]
        time.sleep(3)
    raise RuntimeError("probe failed: " + last)

def show(label, fp, keys=("userAgent", "platform", "uaDataPlatform", "languages", "timezone", "screen", "cores", "memory", "webdriver", "webglVendor", "webglRenderer", "canvasHash", "webrtc", "cookie", "url")):
    print(f"--- {label}")
    for k in keys:
        if k in fp and fp[k] is not None:
            print(f"    {k}: {fp[k]}")

RESULTS = []
def check(name, ok, detail=""):
    RESULTS.append((name, bool(ok)))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""))

def summary():
    failed = [n for n, ok in RESULTS if not ok]
    print(f"\n{len(RESULTS) - len(failed)}/{len(RESULTS)} checks passed" + (f"; FAILED: {failed}" if failed else ""))
    return not failed

def wait_api(timeout=600):
    """Block until the app answers on its Octo-compatible API."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if api("GET", "/api/profiles/active")[0] == 200:
                return True
        except Exception:
            pass
        time.sleep(3)
    raise RuntimeError("the app is not answering on " + BASE)

def note(text):
    print("NOTE " + text)

def public_ip():
    """The address this machine reaches the internet from, without any proxy."""
    with _opener.open("https://api.ipify.org", timeout=20) as r:
        return r.read().decode().strip()

# Names the two engines use for one zone: Chromium's ICU keeps the older ones.
_TZ_ALIASES = {"Asia/Saigon": "Asia/Ho_Chi_Minh", "Asia/Calcutta": "Asia/Kolkata", "Europe/Kiev": "Europe/Kyiv", "Asia/Katmandu": "Asia/Kathmandu", "Asia/Rangoon": "Asia/Yangon"}

def same_tz(a, b):
    return a is not None and b is not None and _TZ_ALIASES.get(a, a) == _TZ_ALIASES.get(b, b)

def expected(name):
    """Comma-separated expectations from the environment, e.g. EXPECT_TZ."""
    value = os.environ.get(name, "").strip()
    return [v.strip() for v in value.split(",") if v.strip()]
