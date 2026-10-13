"""Interface language: a page sees one language everywhere, and a profile that asks for another interface language shows it only where expected.

By default the browser's interface follows the profile's language, so
`navigator.languages`, `Intl`, date formats and form messages agree. With
`interface_language` set, the page still gets the profile's
`navigator.languages`; what else gives the interface away depends on the
engine and is checked here as measured.

Says most with SCN_REMOTE_PROXY=http://user:password@host:port, an exit in a
country whose language is not this machine's; without it the profiles speak
the local exit's language.
"""
from scn import *
from urllib.parse import urlparse, unquote
REMOTE = os.environ.get("SCN_REMOTE_PROXY", "").strip()
ASKED = os.environ.get("SCN_INTERFACE_LANGUAGE", "ru")
wait_api()

def lang(tag):
    return (tag or "").replace("_", "-").split("-")[0].lower()

def read(uuid, label):
    status, st = start(uuid)
    check(f"{label}: starts", status == 200, f"{status} {str(st)[:160]}")
    if status != 200: return None
    time.sleep(4)
    fp = probe(st, url="https://example.com")
    show(label, fp, keys=("languages", "locale", "numberLocale", "dateSample", "formMessage", "timezone"))
    stop(uuid); time.sleep(3)
    return fp

created, proxies = [], []
try:
    proxy = None
    if REMOTE:
        remote = urlparse(REMOTE)
        body = {"title": f"S14 remote {int(time.time())}", "type": "http", "host": remote.hostname, "port": remote.port}
        if remote.username: body.update(login=unquote(remote.username), password=unquote(remote.password or ""))
        status, r = api("POST", "/api/v2/automation/proxies", body)
        assert status == 200, (status, r)
        proxy = r["data"]["uuid"]; proxies.append(proxy)
    else:
        note("no SCN_REMOTE_PROXY: the profiles speak this machine's exit language")

    status, r = api("POST", "/api/v2/automation/profiles", {"title": "S14 bad tag", "fingerprint": {"os": "win"}, "interface_language": "(ru)"})
    check("an interface language that is not a language tag is refused", status == 400, f"{status} {str(r)[:120]}")
    if status == 200: created.append(r["data"]["uuid"])

    for browser in ("camoufox", "cloakbrowser"):
        own = create(f"S14 {browser} own language", "win", browser=browser, proxy=proxy); created.append(own)
        fp = read(own, f"{browser}, interface in the profile's language")
        if not fp: continue
        spoken = lang(fp["languages"][0])
        check(f"{browser}: Intl speaks the profile's language", lang(fp["locale"]) == spoken, f"{fp['locale']} beside {fp['languages']}")
        check(f"{browser}: numbers are formatted in the profile's language", lang(fp["numberLocale"]) == spoken, fp["numberLocale"])
        own_message, own_date = fp["formMessage"], fp["dateSample"]

        again = read(own, f"{browser}, second launch")
        if again:
            check(f"{browser}: the same at the next launch", (again["languages"], again["locale"], again["formMessage"]) == (fp["languages"], fp["locale"], fp["formMessage"]), f"{again['languages']} {again['locale']}")

        if spoken == lang(ASKED):
            note(f"{browser}: the profile already speaks {ASKED}; another interface language cannot be told apart here")
            continue
        other = create(f"S14 {browser} interface {ASKED}", "win", browser=browser, proxy=proxy, interface_language=ASKED); created.append(other)
        fp = read(other, f"{browser}, interface in {ASKED}")
        if not fp: continue
        check(f"{browser}: navigator.languages stay the profile's", lang(fp["languages"][0]) == spoken, str(fp["languages"]))
        check(f"{browser}: the interface language reaches form messages", fp["formMessage"] != own_message, fp["formMessage"])
        status, st = api("GET", "/api/v2/automation/profiles/" + other)
        pro = browser == "cloakbrowser" and not st["data"]["version"].startswith("145.")
        if pro:
            check("cloakbrowser Pro: Intl stays the profile's under another interface language", lang(fp["locale"]) == spoken, fp["locale"])
        else:
            # Measured, not wanted: these builds take Intl from the interface.
            check(f"{browser}: Intl follows the interface language on this build", lang(fp["locale"]) == lang(ASKED), fp["locale"])
finally:
    for u in created: stop(u)
    time.sleep(2)
    if created: delete(*created)
    if proxies: api("DELETE", "/api/v2/automation/proxies", {"uuids": proxies})
summary()
