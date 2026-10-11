# Launch scenarios

End-to-end checks that drive a running Antidetect dev app through its
Octo-compatible API (`127.0.0.1:58888`) and read the device a site would see out
of the launched browsers (WebDriver BiDi for Camoufox, CDP for CloakBrowser).

Run the app without proxy variables in its environment, then run scenarios from
this directory. `--no-watch` keeps the app from restarting under a run when a
source file changes:

```bash
env -u HTTP_PROXY -u HTTPS_PROXY pnpm tauri dev --no-watch
```

Browsers start headless, so a run does not take over the screen; `SCN_HEADED=1`
shows their windows.

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
| `s7_ssh_proxy.py` | Both engines route through an SSH proxy; a wrong password refuses the profile. Needs `cargo build --example ssh_test_server` in `src-tauri`. |
| `s9_cookie_robot.py` | Cookie Robot on both engines: bad requests refused, sites visited and counted, profiles closed afterwards with their cookies kept, a running robot stopped. |
| `s10_one_time.py` | One-time profiles: started in one call, absent from the profile list, deleted when stopped and when their browser exits on its own. |
| `s8_remote_exit.py` | Through a real remote proxy (`SCN_REMOTE_PROXY=http://user:password@host:port`): sites see the proxy's address and timezone, WebRTC shows nothing of this machine, the language is the same at every launch, and a profile whose exit moves follows it at the next launch. Skipped without the variable. |

`EXPECT_TZ` and `EXPECT_LANG` (comma-separated) add geolocation assertions to
s1 and s3, for example `EXPECT_TZ=Europe/Berlin EXPECT_LANG=de`.

The scenarios create and delete their own profiles and proxies; s1 leaves its
three profiles running on purpose and removes the previous run's on start.
