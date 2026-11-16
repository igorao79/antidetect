"""A session the kill switch cut comes back by itself once its proxy answers
from the same place, and stays cut when the proxy comes back from elsewhere.

Needs the dev app with "Block launches without a proxy or VPN" on
(`require_route_for_launch` in its app_settings.json). The moved-exit half
needs SCN_REMOTE_PROXY (an http://user:pass@host:port proxy whose exit is in
another country than this machine); without it that half is skipped.
"""
from scn import *

LOG_FILE = os.path.expanduser("~/Library/Logs/com.company.antidetect/AntidetectDev.log")
PORT = 18766
PROXY_LOG = os.path.join(HERE, "proxy.log")
REMOTE = os.environ.get("SCN_REMOTE_PROXY", "").strip()


def log_size():
    return os.path.getsize(LOG_FILE) if os.path.exists(LOG_FILE) else 0


def wait_log(since, needle, timeout):
    """The first app log line after `since` that contains `needle`."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        with open(LOG_FILE, "rb") as f:
            f.seek(since)
            for line in f.read().decode(errors="replace").splitlines():
                if needle in line:
                    return line
        time.sleep(2)
    return None


def start_proxy(upstream=None):
    args = ["node", "test-proxy.mjs", str(PORT), PROXY_LOG] + ([upstream] if upstream else [])
    proc = subprocess.Popen(args, cwd=HERE)
    time.sleep(1.5)
    return proc


def page_loads(started, url="https://example.com/", tries=1):
    # A query of its own keeps the answer out of the browser's cache, so a
    # page that loads came over the network.
    url = f"{url}?s19={time.time_ns()}"
    try:
        fp = probe(started, url=url, tries=tries)
        return fp.get("url", "") == url, fp.get("url", "")
    except Exception as e:
        return False, str(e)[:120]


def cut_session(proxy, log_id, started):
    """Kill the proxy and make the browser dial until the app cuts the session."""
    proxy.terminate()
    proxy.wait(5)
    since = log_size()
    for url in ("https://example.com/", "https://example.org/", "https://www.wikipedia.org/"):
        page_loads(started, url)
    return wait_log(since, f"Route down, session cut profile={log_id}", 150), since


wait_api()
proxy = start_proxy()
status, r = api("POST", "/api/v2/automation/proxies",
                {"title": f"S19 local {int(time.time())}", "type": "http", "host": "127.0.0.1", "port": PORT})
proxy_uuid = r["data"]["uuid"]
uuid = create("S19 reconnect", "mac", proxy=proxy_uuid)
log_id = str(__import__("uuid").UUID(uuid))  # the log writes ids with dashes
try:
    since_launch = log_size()
    status, started = start(uuid)
    check("profile starts through the test proxy", status == 200, str(started)[:120])
    first_worker = wait_log(since_launch, f"profile={log_id} upstream=http", 30) or ""
    port = next((part[5:] for part in first_worker.split() if part.startswith("port=")), None)
    note(f"browser's local proxy port {port}")
    # The browser's BiDi endpoint can refuse the first connections after start.
    ok, where = page_loads(started, tries=4)
    check("a page loads before the outage", ok, where)

    cut, since_cut = cut_session(proxy, log_id, started)
    check("the kill switch cuts the session when the proxy dies", cut is not None)
    ok, where = page_loads(started)
    check("nothing goes direct while the session is cut", not ok, where)

    since_back = log_size()
    proxy = start_proxy()
    back = wait_log(since_back, f"Session reconnected profile={log_id}", 120)
    check("the session reconnects once the proxy answers again", back is not None, back or "")
    check("the reconnected worker listens on the browser's port",
          back is not None and port is not None and f"port={port} " in back + " ", back or "")
    ok, where = page_loads(started)
    check("a page loads after the reconnect, without a restart", ok, where)

    if not REMOTE:
        note("SCN_REMOTE_PROXY not set: moved-exit half skipped")
    else:
        cut, _ = cut_session(proxy, log_id, started)
        check("a second outage cuts the session again", cut is not None)
        since_moved = log_size()
        proxy = start_proxy(REMOTE)
        stopped = wait_log(since_moved, f"Reconnect stopped profile={log_id}", 150)
        check("an exit in another country is not reconnected",
              stopped is not None and "PROXY_EXIT_MOVED" in stopped, stopped or "")
        ok, where = page_loads(started)
        check("the session stays cut after the refusal", not ok, where)
finally:
    if proxy.poll() is None:
        proxy.terminate()
    stop(uuid)
    time.sleep(2)
    delete(uuid)
    api("DELETE", "/api/v2/automation/proxies", {"uuids": [proxy_uuid]})
summary()
