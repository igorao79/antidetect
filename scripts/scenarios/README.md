# Launch scenarios

End-to-end checks that drive a running Antidetect dev app through its
Octo-compatible API (`127.0.0.1:58888`) and read the device a site would see out
of the launched browsers (WebDriver BiDi for Camoufox, CDP for CloakBrowser).

Run the app without proxy variables in its environment, then run scenarios from
this directory:

```bash
env -u HTTP_PROXY -u HTTPS_PROXY pnpm tauri dev
```

```bash
python3 s1_camoufox_fingerprints.py
```

| Scenario | What it checks |
| --- | --- |
| `s1_camoufox_fingerprints.py` | Three Camoufox profiles (Windows, macOS, Linux): OS-consistent device, engine version in the UA, WebRTC address, distinct canvas. Leaves them running for s2 and s4. |
| `s2_camoufox_persistence.py` | Same device and cookies after a restart; debug ports recovered for running profiles. |
| `s3_cloakbrowser.py` | CloakBrowser profiles: OS and client hints, per-seed noise, same device and cookies after a restart. |
| `s4_api_edge_cases.py` | Status codes for bad requests, Host and content-type guards, headless start, delete. |
| `s5_proxy_routing.py` | A profile's traffic goes through its assigned proxy (a local logging proxy). |
| `s5b_dead_proxy.py` | A proxy that dies never makes a profile fall back to a direct connection. |
| `s6_bot_detection.py` | `bot.sannysoft.com` reports no failed test for either engine. |

`EXPECT_TZ` and `EXPECT_LANG` (comma-separated) add geolocation assertions to
s1 and s3, for example `EXPECT_TZ=Europe/Berlin EXPECT_LANG=de`.

The scenarios create and delete their own profiles and proxies; s1 leaves its
three profiles running on purpose and removes the previous run's on start.
