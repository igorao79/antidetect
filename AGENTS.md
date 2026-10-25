# Antidetect (internal fork of Donut Browser)

This repository is the company's internal fork of [Donut Browser](https://github.com/zhom/donutbrowser)
(AGPL-3.0), branded "Owl" (it was "Antidetect" before) and used only inside the company. The goal is feature parity
with Octo Browser. Upstream is kept as the `upstream` remote and `fork-base` tags the commit the
fork started from; merge upstream regularly. The published history starts from one commit that
holds Donut Browser as it was at `fork-base` (upstream's own history is left out); before merging
upstream, give that commit its real parent locally so Git sees the shared ancestry:
`git fetch upstream --tags && git replace --graft $(git rev-list --max-parents=0 HEAD) fork-base`.
Files of upstream's that the fork does not use (CI and release workflows, Linux and Windows
packaging, Nix, community docs, changelog, template images) are removed.

Fork-specific rules:

- Paid-plan gates on local features (API/MCP automation, any fingerprint OS) are removed in
  `cloud_auth.rs` and `src/lib/entitlements.ts`. Features that need Donut's cloud stay off.
- Engines: Camoufox (anti-detect Firefox, free, restored from upstream `23dab4c8e4`) is the
  engine; Wayfern stays available for a paid subscription and is only downloaded on demand.
  Only Camoufox is fetched at startup. The product is to stay free, so CloakBrowser (Chromium,
  with a paid Pro tier) was removed in October 2026, with its camera video, licence and
  Chromium language override. Its profiles stay listed so their data can be exported; a
  launch answers `ENGINE_REMOVED`, and the downloaded build is cleaned up once no profile uses
  it.
- `octo_compat_api.rs` serves an Octo Browser compatible API on `127.0.0.1:58888` (no token,
  loopback only): profile start/stop/active, one-time profiles (`/api/profiles/one_time/start`,
  deleted when their browser stops) plus Octo's automation calls against local data: profiles
  (list with `search`, `search_tags`, `page`/`page_len`; read; create; `PATCH` of title,
  description, tags and proxy; delete; `import_cookies`, and `GET .../cookies` as an extension),
  proxies (list, create, `PATCH`, delete) and `GET /tags`.
- Cookie Robot (`cookie_robot.rs`, `cookie-robot-dialog.tsx`, "Warm up cookies" in the selection
  bar's menu) is the local counterpart of Donut's cloud Cookie Bot: it launches each selected
  profile, walks a site list over CDP or WebDriver BiDi with pauses and scrolling, and closes it.
  One run at a time; also `GET/POST/DELETE /api/v2/automation/cookie_robot` on the local API.
- Profile templates (`profile_templates.rs`, `profile-template-menu.tsx`, the "Template" pill
  of the create dialog) keep the dialog's choices under a name in
  `data/profile_templates.json`: engine, proxy, extension group and fingerprint settings, never
  a pinned fingerprint or seed, so every profile made from one is still its own device. A
  template saved for an engine the app no longer has keeps the dialog's current engine.
- The synchronizer takes Camoufox profiles as well as Wayfern ones, as leader and follower,
  mixed or not (`synchronizer_bidi.rs`). Camoufox speaks WebDriver BiDi only: its leader is
  watched by a preload script in a sandbox the page's own scripts cannot see, handing the
  user's trusted mouse, key and wheel events to a `script.message` channel; the browser runs it
  again in every new document, so navigations re-arm it. Top-level `navigationStarted` stands
  for an address-bar navigation. Followers are replayed with `input.performActions` and
  `browsingContext.navigate` into the tab in front, and placed with
  `browser.setClientWindowState`. Pause and holding a follower out work as on Wayfern; the
  three window layouts go through the same call but have not been tried end to end. Input
  inside frames is not mirrored. Firefox takes one BiDi
  session per browser and keeps it after its connection drops, so while a profile is in a
  session nothing else can automate it, and every Camoufox stop ends the synchronizer's
  session first (`synchronizer_bidi::release`) so the quit stays a clean one. Also on the
  local API as `POST/DELETE /api/v2/automation/synchronizer`, where `headless` runs a session
  without windows; debug builds add `POST /api/v2/automation/synchronizer/bidi`, which sends
  a BiDi command through the held session (how `s12_synchronizer.py` drives and reads it).
- A persistent Camoufox profile reopens its session at launch: tabs, their session storage
  and session cookies (`SESSION_RESTORE_PREFS` in `camoufox_manager.rs`, written to `user.js`
  at every launch). camoufox.cfg starts on a blank page and drops the session. A stop first
  asks the browser to quit over WebDriver BiDi (`close_over_bidi`: `Services.startup.quit` in
  the parent process, which Camoufox is launched with `--remote-allow-system-access` for), so
  it writes its session, cookies and storage, and falls back to SIGTERM and a kill when it
  cannot ask (an automation client holds Firefox's one BiDi session). BiDi's own
  `browser.close` is not a quit: it closes the tabs one by one, the session records each
  closure, and on macOS the browser stays running without windows until the kill, so only
  the last tab or none came back. After a clean quit Firefox
  reopens tabs and session cookies but drops session storage, which only a crashed session
  brings back, so each launch moves `sessionstore.jsonlz4` to
  `sessionstore-backups/recovery.jsonlz4` (`resume_as_recovery`) and the session resumes as
  after a crash, without the restore prompt. Ephemeral profiles keep their no-history prefs.
  `s16_session_restore.py` checks it; session storage comes back a moment after the
  restored page loads, and a value set from outside over BiDi is not recorded at all.
- Own Owl server (`donut-sync/`, `workspace.rs`, `workspace-page.tsx`,
  `workspace-share-dialog.tsx`, the "Server" page in the rail). Anyone runs one with
  `docker compose up` in `donut-sync/` (one container, objects on its own disk in the
  `owl-data` volume, `STORAGE=disk`; S3 stays possible) and invites people; the first start
  logs the owner's `owl://join?server=..&code=..` link (`node dist/cli owner-link` prints a new
  one while there is no owner). Members join by link with a name and get a personal token,
  kept as the self-hosted sync token, so the existing sync engine does the syncing.
  Server side: `node:sqlite` members/invites/profile authors in `owl.db`, member tokens and
  invite codes stored as SHA-256, `POST /v1/join`, `/v1/me`, `/v1/members`, `/v1/invites`,
  disk transfers at `/v1/blob/<key>` signed by the server, and profile locks at
  `/v1/profile-locks` in the Donut cloud's lock protocol, in memory with a 120 s TTL and
  announced on the subscribe stream as `lock` events. `team_lock.rs` sends its calls through
  `workspace::lock_request`, so a joined server's locks work like the cloud's. A profile shared
  there carries `shared_by` (member id and name) in its synced metadata; only that member or
  the owner turns its sharing off or deletes it (`workspace::may_unshare`, and the server
  refuses another member's `delete-prefix` of `profiles/<id>/`). Shared profiles appear on
  other members' machines as soon as the server announces them; Firefox lock files and
  `user.js` are not synced. Password encryption is not used for it yet.
- The Owl server has an API for scripts (`workspace_api.rs`, `workspace-api-panel.tsx`, the
  "Server API" tab in Integrations): the owner creates keys (`owlk_…`, `POST /v1/api-keys`,
  only the SHA-256 on the server, the key itself sealed in the creating app under
  `workspace-api-keys/` so it can be copied again and checked). A key acts as the owner for
  members, invites, `GET /v1/profiles` (the shared profiles read from their synced
  `metadata.json`, with `shared_by` and who has each open) and `GET /v1/profile-locks`; the
  auth guard refuses it for `/v1/objects*` (synced data), `/v1/api-keys` and taking a lock.
- An app syncs with one Owl server at a time and keeps every server it joined
  (`workspace_servers.rs`, `workspace-server-list.tsx`, "Add server" on the Server page):
  `workspaces.json` lists them, each member token sealed in its own file under
  `workspace-tokens/`. Joining another server, or switching, parks what the server being
  left syncs: profiles, proxies, groups, VPNs, extensions and extension groups with sync on
  get it turned off locally (no `updated_at` bump, nothing sent) and are listed in that
  server's entry; switching back turns it on again, so nothing of one server reaches another.
  Leaving a server forgets it and leaves its entities as local copies with sync off. A switch
  or a leave waits while a shared profile is open (`WORKSPACE_PROFILES_RUNNING`).
- The Server page also installs an Owl server on a VPS (`workspace_deploy.rs`,
  `workspace-deploy-form.tsx`): the app logs in over SSH (password, or a key file; root, or
  sudo with or without the password), uploads the server's sources it carries
  (`build_owl_server.rs` packs `donut-sync/` into `OUT_DIR/owl-server.tar` at build time) and
  runs `workspace_deploy.sh` as root. The script needs Debian or Ubuntu, installs Docker
  (get.docker.com, the distribution's packages when that refuses a release), writes
  `/opt/owl-server/donut-sync/.env` (keeping settings it does not manage), opens ufw, starts
  the compose file with the `https` profile and prints the owner link. When nginx already
  holds ports 80/443 (a VPS with a site on it), Caddy stays off: the script adds its own
  nginx site for the domain (`owl-server.conf`, kept only if `nginx -t` accepts it) and runs
  `certbot --nginx` for it, leaving the other sites alone; any other program on those ports is
  refused before anything is installed (`WORKSPACE_DEPLOY_PORTS_BUSY`). What it installed
  (Docker, certbot, the nginx site, the certificate) is listed in `/opt/owl-server/installed`.
  Without a domain the server is named `<ip-with-dashes>.sslip.io`, so a certificate comes
  with no DNS setup; a private address is refused. The app then waits for HTTPS and joins as the owner. Running it
  again updates the server and keeps its data (`WORKSPACE_DEPLOY_HAS_OWNER` once it has an
  owner). Host keys are accepted and logged, not pinned. The E2E test of a real install runs
  only with `OWL_DEPLOY_SSH`/`OWL_DEPLOY_KEY` pointing at a disposable machine (a Lima VM),
  over plain HTTP, which only debug builds allow.
- Donut's cloud is switched off in the interface by one flag, `DONUT_CLOUD` in
  `src/lib/cloud-only-pages.ts`, with the lists beside it: the Account page (signing in, sync
  setup), the cloud Cookie Bot and the Agent console are out of the rail, the shortcuts and
  the command palette; Settings has no Donut licence and no sync encryption section; the
  welcome has no licence step; the trial-expired and paid-welcome dialogs never open; no table
  shows a sync column, filter or bulk sync button, and a profile's details have no sync
  section; tips that lead to those pages are not listed. Upstream's code and tests stay for
  merges (the E2E tests of those surfaces are skipped with `CLOUD_ONLY`).
- Wayfern takes its fingerprints and launch token from a Donut account, so it is offered only
  where a build is already installed (`donut_cloud.rs::wayfern_offered`, applied in
  `get_supported_browsers`; the create dialog asks that command). With none installed the
  app no longer asks donutbrowser.com for the Wayfern version at every start. Signed out,
  nothing else in the backend calls Donut's servers.
- Owl updates itself from the fork's GitHub releases (`RELEASES_REPO` in
  `app_auto_updater.rs`; upstream's updater, which checks at start and every 3 hours, downloads
  the .dmg, verifies it against the release's `SHA256SUMS.txt` and offers a restart). Only a
  build made with `STABLE_RELEASE` (version `v<x.y.z>`) takes updates; a plain `pnpm tauri
  build` calls itself `dev-<x.y.z>` and never updates. To ship: bump the version in
  `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, commit, push, and run
  `scripts/release-macos.sh`, which builds, writes `SHA256SUMS.txt` and publishes the release.
  Publishing is public (the repository is public): ask before running it. Settings → Updates
  turns the automatic checks off (`disable_auto_updates`, on by default) and has a "Check for
  updates" button (`check_for_app_updates_manual`), whose find goes to the same download and
  restart toast through an `app-update-available` event. A release is signed
  with the Developer ID certificate in the keychain and notarized (the app by Tauri with its
  ticket stapled, then the .dmg by `notarytool` and stapled), so macOS opens it without a
  warning and keeps camera and microphone grants across updates; the identity and the App
  Store Connect API key come from `~/.config/owl/release.env`, outside the repository, and the
  script refuses to publish without them. Local builds stay ad-hoc signed (`signingIdentity`
  `-` in `tauri.conf.json`).
- The camera and microphone prompt at launch is offered once (`src/lib/permission-prompt.ts`);
  Settings requests them afterwards. An ad-hoc signed build is a new app to macOS after every
  rebuild, so a grant given to an earlier build does not carry over.
- Bundled fonts on macOS (`camoufox/bundled_fonts.rs`). Camoufox carries the fonts of every OS
  it poses as in `Resources/fonts` (`gfx.bundled-fonts.activate`), among them other versions of
  fonts this Mac has (SF, Helvetica, Arial, Georgia: 476 files on macOS 26). A profile posing as
  another OS (`navigator.platform` not a Mac one) shaped text with the bundled file while
  WebRender drew the installed font's glyphs, so tabs, the address bar and fallback text such
  as Cyrillic showed other characters. Before a launch those files move to
  `Resources/fonts-shadowed` once per build and macOS version (`.owl-checked` holds the build);
  fonts the Mac lacks (Calibri, Segoe UI) stay bundled. `drawWindow` renders correctly either
  way, so only a screenshot of the window shows it.
- Address-bar search (`camoufox/search.rs`). The Camoufox build has no search engines: its
  configuration carries one inert engine, "None", at `http://127.0.0.1/`, and its
  `distribution/policies.json` makes it the default, so a search typed in the address bar
  went to 127.0.0.1. Every launch rewrites the policy's `SearchEngines` (Google, the default,
  Bing, DuckDuckGo, Yandex, Wikipedia; "None" hidden). Firefox applies the policy's default
  once per change, so a person may pick another engine, and older profiles switch too.
  `s18_search_engine.py` checks it.
- Interface language (`interface_language.rs`, `interface-language-field.tsx`, the
  `interface_language` setting of a Camoufox profile, also on the local API's profile
  creation). Camoufox keeps no language of its menus apart from what a page reads, so by
  default the interface follows the profile's language. It takes menus, `Intl`, date formats
  and form messages from `intl.locale.requested`, handed over as `CAMOU_PREFS_1`; its
  translations are inside the build. A profile may ask for another interface language (the
  app's): `navigator.languages` stay the profile's, and form messages and `Intl` turn to that
  language.
- SSH is a proxy type (`ssh://user:password@host:port`, password or keyboard-interactive
  login). `ssh_upstream.rs` runs inside the local proxy worker: one session per upstream, one
  `direct-tcpip` channel per browser connection, TCP only. Host keys are not pinned.
- Put fork code in its own modules and keep edits to upstream files small, so upstream merges
  stay easy. Fork modules: `browser_runner/{launch_network,engine_common,camoufox}.rs`,
  `api_client/camoufox.rs`, `camoufox/`, `camoufox_manager.rs`,
  `profile_templates.rs`, `interface_language.rs`, `donut_cloud.rs`,
  `profile/{after_exit,camoufox}.rs`, `ssh_upstream.rs`,
  `cookie_robot.rs`, `octo_compat_api.rs`, `synchronizer_bidi.rs`, `workspace.rs`,
  `workspace_deploy.rs` (with `workspace_deploy.sh` and `build_owl_server.rs`),
  `workspace_servers.rs`, `workspace_api.rs`, and on the
  frontend `camoufox-config-dialog.tsx`, `shared-camoufox-config-form.tsx`,
  `cookie-robot-dialog.tsx`, `profile-template-menu.tsx`, `interface-language-field.tsx`,
  `workspace-page.tsx`, `workspace-share-dialog.tsx`, `workspace-deploy-form.tsx`,
  `workspace-server-list.tsx`, `workspace-api-panel.tsx`; on the server `donut-sync/src/storage/`,
  `donut-sync/src/workspace/` and `donut-sync/src/cli.ts`.
- Keep the AGPL license and upstream copyright notices.
- Commit only when the user asks.

What testing established about the engines (`scripts/scenarios/` reruns these checks against a
running dev app):

- Camoufox 156 surfaces, measured on the binary with two profiles that differ only in their
  seeds and offsets (`s13_fingerprint_surfaces.py` reruns it through the app): `toDataURL`,
  `toBlob` and the WebGL image carry stock Firefox's per-session randomization
  (`EfficientCanvasRandomization` of baseline fingerprinting protection), so profiles are not
  linkable by them but a profile's value changes at every launch. `getImageData`, WebGL
  `readPixels` and text metrics are this machine's, the same in every profile: `canvas:seed`,
  `canvas:aaOffset` and `fonts:spacing_seed` do not reach them in this build (the canvas
  anti-aliasing patch is not in the open-source build), with GPU canvas on or off. The audio
  fingerprint follows `audio:seed`: each profile's own and stable. Switching the randomization
  off (`privacy.baselineFingerprintingProtection` false, or `-EfficientCanvasRandomization` in
  the overrides) makes `toDataURL` stable but the machine's own, shared by every profile, so
  the app leaves it on; a canvas that is a profile's own and stable needs a patch to the
  engine.
- Launch the dev app without `HTTP(S)_PROXY` in its environment: the app's own lookups honour
  those variables while a profile's traffic does not, so geolocation ends up for the wrong exit.
- Location follows the exit at every launch: the exit address is measured
  through the launch's local proxy and sets timezone, language, coordinates and the WebRTC
  address (unless the profile turned automatic location off). A new Camoufox fingerprint is
  located through a proxy check, never by handing the proxy to reqwest, which looks up this
  machine's own address for every type it cannot dial (SSH, Shadowsocks, share links).
- A profile speaks the main language of its exit country: the most spoken of its official
  languages (`geolocation::get_profile_geolocation`). Upstream's draw across CLDR speaker
  shares counts second languages and dialects (a German exit came out `de` 37% of the time,
  `en` 26%, `bar` 7%) and is kept only for Wayfern.
- A launch whose location follows the exit waits for the GeoIP database (a fresh install is
  still downloading it) and fails with `LOCATION_DATABASE_UNAVAILABLE` after 150 s. Without the
  database the browser came up with this machine's own timezone and a default language.
- Before the interface language was set at launch, a page could read a language the profile
  did not speak: Camoufox reported `Intl` as `en-US` under any `navigator.language`.
  `s14_interface_language.py` holds the fix; the earlier scenarios only compared
  `navigator.languages` and the timezone.
- The REST API (`api_server.rs`) asked for the Wayfern terms on every install and answered
  403 before it looked at the key, so on an install without Wayfern (every fork install) the
  API on port 10108 refused everything and "Check connection" read "key not accepted". The
  terms now gate the API only where Wayfern is installed, the rule the interface already
  used. The E2E harness seeds accepted terms, which is why no suite saw it.
- The first emoji a Camoufox session draws on a canvas uses a fallback glyph while Firefox
  loads the emoji font; that one reading differs from every later one. Shapes and Latin text
  are stable from the start, so this is font loading and not canvas noise.
- The native E2E suites run on this fork: smoke, entities, integrations and ui pass without
  credentials. Their expectations follow the fork (rebranded strings, no paid gates, Wayfern
  picked in the create dialog because it is the engine the harness registers), and the fork's
  own surfaces have tests there (Cookie Robot, SSH proxy form, profile series, command
  contracts). Three ui tests ask for a 900 px tall window and cannot pass on a 1280x832
  display; `browser` needs `WAYFERN_TEST_TOKEN`, `network` and `sync` need Docker.
- After a browser exits, `profile/after_exit.rs` drops an ephemeral profile's data, applies
  clear-on-close and deletes a temporary profile, on a stop and when the user closes the
  window. Wayfern has the same steps inline in `browser_runner.rs`.
- A debug build with full settings takes 11 GB and more per flavour. `CARGO_INCREMENTAL=0
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` brings it to about 3.5 GB; with the
  frontend built (`pnpm build`), `src-tauri/target/debug/donutbrowser` runs without `tauri dev`.

---

# Project Guidelines

> NOTE: CLAUDE.md is a symlink to AGENTS.md. Editing either file updates both.
> After significant changes (new modules, renamed files, new directories), re-evaluate the Repository Structure below and update it if needed.

## Repository Structure

```
donutbrowser/
├── src/                              # Next.js frontend
│   ├── app/                          # App router (page.tsx, layout.tsx)
│   ├── components/                   # 50+ React components (dialogs, tables, UI)
│   │   ├── table-controls.tsx         # Shared filters, sort rules, column controls, and row selection
│   │   ├── managed-data-table.tsx     # Shared management table and keyboard navigation
│   │   └── tips/                     # Feature tips: SVG scene primitives and one looping scene per tip
│   ├── hooks/                        # Event-driven React hooks, table preferences, filtering, and keyboard controls
│   ├── i18n/locales/                 # Translations (de, en, es, fr, ja, ko, pt, ru, tr, vi, zh)
│   ├── generated/                    # Build-generated third-party license inventory
│   ├── lib/                          # Utilities (themes, toast, browser-utils)
│   └── types.ts                      # Shared TypeScript interfaces
├── src-tauri/                        # Rust backend (Tauri)
│   ├── src/
│   │   ├── lib.rs                    # Tauri command registration (100+ commands)
│   │   ├── browser_runner.rs         # Profile launch/kill orchestration
│   │   ├── browser.rs               # Browser trait & launch logic
│   │   ├── profile/                  # Profile CRUD (manager.rs, types.rs)
│   │   ├── proxy_manager.rs         # Proxy lifecycle, connection testing, per-proxy check history
│   │   ├── proxy_udp.rs             # SOCKS5 UDP ASSOCIATE probe (yes/no/unknown UDP verdict)
│   │   ├── proxy_server.rs          # Local proxy binary (donut-proxy)
│   │   ├── ssh_upstream.rs          # SSH proxies: shared session, direct-tcpip channels (fork)
│   │   ├── proxy_storage.rs         # Proxy config persistence (JSON files)
│   │   ├── api_server.rs            # REST API (utoipa + axum)
│   │   ├── mcp_server.rs            # MCP tool engine, served over the remote bridge
│   │   ├── mcp_remote.rs            # Remote MCP bridge: outbound websocket to Donut cloud (Enterprise remote control)
│   │   ├── mcp_integrations.rs      # 20-client MCP installer: remote endpoint with bearer, format-preserving JSONC/TOML edits
│   │   ├── automation_rate_limiter.rs # Shared REST/MCP automation quota
│   │   ├── agent_console.rs         # The person's side of connected MCP agents: activity, questions, notes, take-over, pause
│   │   ├── recipes.rs               # Thin client for cloud recipes; storage and replay stay in the cloud
│   │   ├── sync/                    # Cloud sync (engine, encryption, manifest, scheduler)
│   │   ├── vpn/                     # WireGuard tunnels
│   │   ├── xray/                    # Xray share links (VLESS, VMess, Trojan, Hysteria2): parse, export, client config
│   │   ├── xray_worker_runner.rs    # Xray-core sidecar lifecycle
│   │   ├── xray_worker_storage.rs   # Private Xray worker state and runtime files
│   │   ├── wayfern_manager.rs       # Wayfern (Chromium) browser management
│   │   ├── downloader.rs           # Browser binary downloader
│   │   ├── extraction.rs           # Archive extraction (zip, tar, dmg, msi)
│   │   ├── settings_manager.rs     # App settings persistence (atomic writes), tips + paid-welcome state
│   │   ├── system_proxy.rs         # The OS proxy Donut's own requests use; named in the error when it is unreachable
│   │   ├── vault.rs                # Per-install key that seals local secrets; opens legacy build-password seals once
│   │   ├── cookie_manager.rs       # Cookie import/export
│   │   ├── cookie_robot.rs         # Local Cookie Robot: warms profiles on a site list (fork)
│   │   ├── profile_templates.rs    # Named sets of the create dialog's choices (fork)
│   │   ├── interface_language.rs   # Language of a profile browser's own menus, per engine (fork)
│   │   ├── donut_cloud.rs          # What the backend still treats as available without Donut's cloud (fork)
│   │   ├── profile_importer.rs     # Bulk profile import (Chromium-family detection, ZIP, batch)
│   │   ├── fingerprint_consistency.rs # Launch-time proxy exit vs fingerprint timezone/language check
│   │   ├── dns_blocklist.rs         # Hagezi DNS blocklists + user custom lists/allowlist
│   │   ├── traffic_stats.rs         # Per-profile traffic stats + secure history erase
│   │   ├── extension_manager.rs    # Browser extension management
│   │   ├── extension_fetch.rs      # Web Store link/id and direct .crx/.zip import, CRX3 unwrapping
│   │   ├── group_manager.rs        # Profile group management
│   │   ├── synchronizer.rs         # Real-time profile synchronizer (pause/resume, hold a follower out, window layouts)
│   │   ├── synchronizer_bidi.rs    # The synchronizer on Camoufox, over WebDriver BiDi (fork)
│   │   ├── workspace.rs            # Own Owl server: join by invite, members, invites, locks routing (fork)
│   │   ├── workspace_deploy.rs     # Install an Owl server on a VPS over SSH and join it as owner (fork)
│   │   ├── workspace_servers.rs    # Joined Owl servers: list, switch, park what the inactive ones sync (fork)
│   │   ├── workspace_api.rs        # Owl server API keys for scripts, kept sealed, and their check (fork)
│   │   ├── daemon/                 # Background daemon + tray icon (currently disabled)
│   │   └── cloud_auth.rs           # Cloud authentication
│   ├── tests/                      # Integration tests
│   └── Cargo.toml                  # Rust dependencies
├── donut-sync/                     # NestJS sync server = the Owl server (docker compose up)
│   └── src/                        # Controllers, services, auth, S3 or disk storage, members/invites/locks
├── e2e/                            # Isolated native UI/sync/Wayfern E2E system
│   ├── app/                        # Test-only Tauri harness that injects the private driver
│   ├── lib/                        # WebDriver, CDP, fixtures, app-session helpers
│   └── tests/                      # Smoke, UI/motion, entity, network, integration, sync, browser suites
├── sdk/                            # Standalone Python + Node clients for the local REST API
│   ├── api-paths.json              # Snapshot of every published operation; drift check for both SDKs
│   ├── python/                     # `donutbrowser` (stdlib only, pytest)
│   └── node/                       # `@donutbrowser/sdk` (ESM TypeScript, node --test)
└── patches/                        # pnpm compatibility patches for secured dependencies
```

## Testing and Quality

- After making changes, run `pnpm format && pnpm lint && pnpm test` at the root of the project
- Always run this command before finishing a task to ensure the application isn't broken
- `pnpm lint` includes spellcheck via [typos](https://github.com/crate-ci/typos). False positives can be allowlisted in `_typos.toml`
- The full `pnpm test` output dumps every test name (≈400+ lines) which burns context for no signal. Filter:
  `pnpm test 2>&1 | grep -E "test result|panicked|FAILED"`. Four "test result: ok" lines means everything passed.

### Native app E2E tests are mandatory for affected behavior

**No E2E suite runs in CI. You are the only thing that runs them.** The `app-e2e` and
`sync-e2e` workflows were removed because they need real credentials, Docker, and a desktop
session that hosted runners could not supply reliably; a permanently red check is worse than
no check. Nothing downstream will catch an E2E regression for you, so skipping the affected
suite means shipping it unverified. Report explicitly which suites you ran and which you did not.

The native suites use the published `tauri-wd` driver (pinned in `e2e/app/Cargo.toml`, installed
into the ignored `e2e/.driver` root) and launch an `e2e`-feature build.
Every session gets its own temporary Donut data/cache/log root, home directory,
WebView store, ports, and sync bucket. Never point a suite at production or development data.
Every suite runs the Donut window headless (`DONUT_E2E_HEADLESS=1`, forwarded as the tauri-wd
`headless` capability), so a run never pops a window or steals focus. `DONUT_E2E_HEADED=1` shows
the window when a failure needs watching.

`e2e/app/Cargo.lock` is generated, gitignored, and never edited by hand. `e2e/run.mjs` seeds it
from `src-tauri/Cargo.lock` whenever that file is newer, so the harness always links the exact
dependency versions Donut ships and a version bump or a Dependabot upgrade needs no second edit.

After a behavior change, run the smallest affected subset below in addition to the standard
format/lint/unit-test command. A code change is not verified until its affected native
suite passes:

| Changed area | Required command |
| --- | --- |
| Startup, settings, persistence, window state, shortcuts, navigation | `pnpm e2e:smoke` |
| React components, dialogs, themes/appearance, responsive layout, accessibility, onboarding | `pnpm e2e:ui` |
| Profile/import/group/proxy/VPN/extension CRUD, DNS, cookies, passwords, traffic | `pnpm e2e:entities` |
| Profile/group/proxy/VPN/extension UI, proxy routing, VPN routing, or their browser-launch integration | `pnpm e2e:network` |
| REST API/OpenAPI, MCP, cloud/update contracts, team locks, real-time synchronizer | `pnpm e2e:integrations` |
| Sync client/server, manifests, timestamps, deletion, encryption, password rollover | `pnpm e2e:sync` |
| Wayfern download/terms/fingerprint, browser runner, CDP, automation endpoints, process cleanup | `pnpm e2e:browser` |
| `donut-sync/` server code (controllers, services, auth, S3 endpoints) | `pnpm --filter donut-sync test:e2e` against the compose S3 server |
| E2E harness, WebDriver plugin/driver, app isolation hooks, or changes spanning multiple rows | Run every affected row; use `pnpm e2e` for cross-cutting changes |

`e2e:browser` requires `WAYFERN_TEST_TOKEN` in the environment or local `.env`. `e2e:network`
and the full suite additionally require Docker plus `RESIDENTIAL_PROXY_URL_ONE_HTTP` and
`RESIDENTIAL_PROXY_URL_ONE_SOCKS`. Other individual suites must run without credentials. Use
`--no-build` only when the frontend, Rust app, sidecar, and WebDriver binaries are already current.
Keep failed artifacts and inspect the per-session app/driver logs and screenshot before changing
assertions.

The `donut-sync` row is the one suite the root `pnpm test` does not cover (`test:sync-e2e` runs
the Rust sync harness only). It needs the S3 server in `donut-sync/docker-compose.test.yml`
(`rclone serve s3` on port 8987; MinIO no longer publishes community images or binaries).
`test/test-env.ts` supplies the matching keys:

```bash
docker compose -f donut-sync/docker-compose.test.yml up -d --wait
pnpm --filter donut-sync test:e2e
docker compose -f donut-sync/docker-compose.test.yml down -v
```

When adding a Tauri command, assign it exactly once in `e2e/coverage-map.mjs` and add executable
evidence to the owning suite. `e2e:smoke` fails if command registration and the coverage map drift.

## Logs (when debugging a running app)

Three log surfaces, in order of usefulness:

- Donut Browser GUI: `~/Library/Logs/com.donutbrowser/DonutBrowser.log` on macOS (newest = active session; older `DonutBrowser_<date>.log` are rotated). The GUI, Tauri, `browser_runner`, `proxy_manager`, and `sync` all log here. Search for `Wayfern`, `Starting local proxy`, `Configured local proxy` to find a launch chain. Dev builds write to `DonutBrowserDev.log` instead.
- donut-proxy worker: `$TMPDIR/donut-proxy-<config_id>.log`. One file per proxy worker process (each profile launch spawns a fresh one). Map a worker to its launch via the `Cleanup: browser PID X is dead, stopping proxy worker <id>` lines in DonutBrowser.log, or by mtime. Upstream rejects (status lines like `HTTP/1.1 402 user reached limit`) and tunnel errors are at WARN. Accepted CONNECT requests are at DEBUG on purpose, so the log is not a list of visited sites; use `RUST_LOG=donut_proxy=debug` to see them. Anything finer is at TRACE and requires `RUST_LOG=donut_proxy=trace`. The `Upstream CONNECT response coalesced N byte(s) of payload` warning (those bytes would be dropped without forwarding) marks a real bug in `handle_connect_from_buffer` if it ever fires.

Linux/Windows swap `~/Library/Logs/com.donutbrowser/` for the platform-appropriate location (see `app_dirs::app_name()`), but the `$TMPDIR` worker logs are always under the system temp dir.

## Code Quality

- Don't leave comments that don't add value
- Don't duplicate code unless there's a very good reason; keep the same logic in one place
- Anytime you make changes that affect copy or add new text, it has to be reflected in all translation files

## Translations (mandatory)

- Never write user-facing strings as raw English literals in JSX, toast messages, dialog titles/descriptions, button labels, placeholders, table headers, tooltips, or empty-state text. Always go through `t("namespace.key")` from `useTranslation()`.
- This applies to every component under `src/`, including new ones. If a component doesn't already import `useTranslation`, add it.
- Adding a new string means adding the key to EVERY locale file in `src/i18n/locales/` (currently de, en, es, fr, ja, ko, pt, ru, tr, vi, zh), not just `en.json`. The English version alone is incomplete work. Don't trust this list: enumerate `src/i18n/locales/*.json` and update every file you find, because a newly added locale is exactly what a hardcoded list silently skips.
- Reuse existing keys (`common.buttons.*`, `common.labels.*`, `createProfile.*`, etc.) before creating new namespaces. Check `en.json` first.
- Strings excluded from this rule: `console.log/warn/error`, dev-only debug labels, internal IDs, CSS class names, type names. If unsure whether a string renders to the user, assume it does and translate it.
- Never use `t(key, "fallback")` with a default-value second argument. The 2-arg form is forbidden: every key must exist in every locale file before the call site lands. Fallbacks mask missing translations, so a key missing from `ru.json` silently renders the English fallback to Russian users and the bug never surfaces in CI or review. Only call `t("namespace.key")`. If a translation is missing for any locale, that's a bug to fix at the JSON, not a hole to paper over at the call site.
- Empty-string values in non-English locales are also forbidden: a locale either has the right translation or it has the same content as English, never `""`. If a particular language doesn't need a particular phrase (e.g. a suffix that doesn't grammatically apply), refactor the JSX to use a single interpolated key (`t("foo.bar", { name })` with `"...{{name}}..."` in each locale) instead of splitting prefix/suffix.
- When adding or removing keys across the locales, use a one-shot Python script in the scratchpad dir that globs `src/i18n/locales/*.json`, mutates each, and writes it back. Sequential `Edit` calls drift (typos, ordering differences) and burn tokens; a single script keeps the locales in lockstep and is easy to throw away. Finish by diffing every locale's flattened key set against `en.json`: zero missing and zero extra, for all of them.

## Backend error codes (mandatory)

User-facing errors returned from a Tauri command MUST be JSON `{ "code": "FOO_BAR", "params": { ... } }` strings, never raw English (`format!("Failed to ...")`). The frontend resolves the code via `translateBackendError(t, err)` from `src/lib/backend-errors.ts`. Adding a new code requires four parallel edits:

1. Emit the JSON from Rust:
   ```rust
   return Err(serde_json::json!({ "code": "FOO_BAR" }).to_string());
   // or with params:
   return Err(serde_json::json!({ "code": "FOO_BAR", "params": { "n": "5" } }).to_string());
   ```
2. Add `"FOO_BAR"` to the `BackendErrorCode` union in `src/lib/backend-errors.ts`.
3. Add a `case "FOO_BAR":` in the switch that returns `t("backendErrors.fooBar", ...)`.
4. Add `backendErrors.fooBar` to every locale file in `src/i18n/locales/`.

Raw error strings reach the user untranslated; that's the bug pattern this rule blocks.

## REST API (`src-tauri/src/api_server.rs`): endpoints must stay in the OpenAPI spec

The served `/openapi.json` comes from the hand-maintained `ApiDoc` derive (`#[derive(OpenApi)]` with `paths(...)`, `components(schemas(...))`, `tags(...)`), NOT from the router. The `OpenApiRouter`-generated spec is discarded (`let (v1_routes, _) = ...`), so a handler registered on the router but missing from `ApiDoc` silently disappears from the spec (this happened to the extension and VPN-export endpoints once).

Any endpoint modification, meaning adding, removing, or changing a route, request/response schema, or status code, must be reflected in the OpenAPI spec in the same change:

1. Keep the handler's `#[utoipa::path]` annotation accurate (path, request body, every reachable response status).
2. Add/remove the handler in `ApiDoc`'s `paths(...)` list and any new schema types in `components(schemas(...))`.
3. Extend the `openapi_*` regression tests in `api_server.rs::tests` (they assert spec coverage and that optional fields stay optional).
4. `#[schema(value_type = Object)]` on an `Option<T>` field erases the optionality and wrongly marks it required. Use `value_type = Option<Object>` (or drop the attribute for natively supported types).

### Error status conventions (known errors)

Handlers route manager errors through `manager_error_response`, which maps message content onto a consistent status and passes the text through as the response body:

- `401`: missing/invalid bearer token (auth middleware; empty body).
- `402`: the five automation endpoints (`run`, `open-url`, `kill`, `batch/run`, `batch/stop`) without a paid plan, and expired-proxy (`PROXY_PAYMENT_REQUIRED`) checks.
- `404`: entity not found (`... not found` / `*_NOT_FOUND`).
- `400`: validation, duplicates, empty names, invalid/unsupported/unavailable input.
- `409`: conflicts, meaning browser version already being downloaded, profile locked by another team member (run), browser running during cookie import.
- `429`: authenticated automation request quota exceeded (`Retry-After` header included).
- `500`: internal failures (IO, network, poisoned locks).

Error bodies are plain-text diagnostics; some are the JSON `{"code": ...}` strings shared with the Tauri commands (e.g. `NAME_CANNOT_BE_EMPTY`, `GROUP_ALREADY_EXISTS`). The translated-error rule above applies to Tauri commands, not to REST bodies.

## Sub-page Dialog mode

A `<Dialog>` becomes a first-class app sub-page (no modal overlay, no center positioning) when `subPage` is passed. Pages like Account, Settings, Proxy Management, and Extension Management use this. The pattern for a sub-page with tabs:

```tsx
<Dialog open={isOpen} onOpenChange={onClose} subPage={subPage}>
  <DialogContent className="max-w-2xl flex flex-col">
    <Tabs defaultValue="account">
      <TabsList
        className={cn(
          "w-full",
          subPage &&
            "!bg-transparent !p-0 !h-auto !rounded-none justify-start gap-4",
        )}
      >
        <TabsTrigger
          value="account"
          className={cn(
            "flex-1",
            subPage &&
              "!flex-none !rounded-none !bg-transparent !shadow-none data-[state=active]:!bg-transparent data-[state=active]:!text-foreground data-[state=active]:!shadow-none text-muted-foreground hover:text-foreground !px-1 !py-1 text-xs",
          )}
        >
          Account
        </TabsTrigger>
        ...
      </TabsList>
      <TabsContent value="account" className="mt-4">...</TabsContent>
    </Tabs>
  </DialogContent>
</Dialog>
```

Reference implementations: `src/components/account-page.tsx`, `src/components/proxy-management-dialog.tsx`. Reuse the exact class strings; the overrides are tuned to match the rest of the sub-page chrome.

### Cross-component tab control

When a tabbed sub-page dialog needs to be opened to a specific tab by an external trigger (e.g. a keyboard shortcut that toggles `proxies` ↔ `vpns`), expose an `initialTab` prop and key the `Tabs` component off it. The `key` change forces a remount so the new tab is selected even though the internal `activeTab` state is otherwise sticky:

```tsx
<AnimatedTabs key={initialTab} defaultValue={initialTab} ...>
```

Reference implementations: `proxy-management-dialog.tsx`, `extension-management-dialog.tsx`, `integrations-dialog.tsx`. The owning page in `src/app/page.tsx` keeps one piece of `useState` per dialog (`proxyManagementInitialTab`, `extensionManagementInitialTab`, `integrationsInitialTab`) and flips it on repeated shortcut presses.

## Feature tips and the paid welcome

Tips are short feature walkthroughs: a looping SVG scene, a title, two or
three lines of copy, and a button into the feature. The catalog is
`src/lib/tips.ts` (ids, deep-link actions, the plan capability a tip needs,
the `inUse` relevance check); scenes live in `src/components/tips/scenes-*.tsx`
and are mapped in `scene-for.tsx`; the dialog is
`src/components/tips-dialog.tsx`; the flow (what to open when) is
`src/hooks/use-tips.ts`. State (`tips_auto_show`, `tips_seen`,
`tips_last_auto_shown_at`, `tips_next_auto_show_at`, `paid_welcome_seen_for`,
`cloud_plan_memory`) is in `AppSettings`, behind the `get_tips_state`,
`mark_tip_seen`, `set_tips_auto_show` and `observe_cloud_plan` commands.

- The automatic tip is one card (`mode: "single"`): no catalog, no pager, and
  its button is Done. The catalog with the pager opens only from the rail,
  the shortcut, the command palette and the paid welcome (`mode: "browse"`).
- It opens only after a settled launch (onboarding done, terms accepted,
  nothing modal open), never in the first-run session. When one opens, the
  backend draws the next time one may open, a random 2 to 5 days later
  (`next_auto_tip_at`), and stores it in `tips_next_auto_show_at`, so a
  restart keeps the same wait. The E2E harness seeds `tips_auto_show: false`;
  a test that wants the automatic tip passes `settings: { tips_auto_show: true }`
  or calls `set_tips_auto_show`.
- The automatic flow picks at random among the unseen tips whose feature is
  not in use (`pickAutoTip`). Each tip's `inUse` reads a `FeatureUsage`
  snapshot: profile fields, groups, extension groups, team, Cookie Bot
  enrolments and the sync server come from data `page.tsx` has loaded; API,
  remote control, fingerprint gate, proxy check history, default
  browser, trash and agent runs are read by `use-tips.ts` through existing
  commands, and only for the tips still in the running. An unknown fact
  never hides a tip. The command palette and import tips have no signal.
- Plan tips carry `requires`; they are listed only when the signed-in plan
  grants the capability. The paid welcome opens once per account when the
  backend sees it turn paid (free -> paid, or a paid account first seen right
  after signing in); `paid_welcome_due` in `settings_manager.rs` is the rule.
- Adding a tip: append to `TIPS` with an `inUse` check when the install
  shows whether the feature is used, write the scene, add
  `tips.items.<id>.{label,title,body,action}` to every locale, and run
  `pnpm test:tips`, which checks every locale carries every tip.
- Scenes are decorative and loop on their own clock (`useScene`); they show
  their resting frame under reduced motion and never hide the copy.

## Timelines (`OperationFlow`)

`src/components/ui/operation-flow.tsx` draws any measured operation as a row
of stations: settled stations wear a check, the current one is a ring (a
cross when `failed`), later ones wait as dots, wires fill as stations settle,
and `busy` sends a pulse along the wire into the station being worked on.
Pass `active` as the station the operation is AT, and `failed` when it
stopped there: a proxy check that cannot connect is `active={1}` (the proxy),
not the device. Reaching the last station with nothing failed settles the row.

## Keyboard shortcuts

All app-wide shortcuts live in `src/lib/shortcuts.ts`:

- `SHORTCUTS[]`: one entry per shortcut (id, label translation key, group, key, modifier flags). The label key must exist in every locale.
- `formatShortcut(s)` returns platform-correct token strings (`["⌘", "K"]` on mac, `["Ctrl", "K"]` elsewhere), used by both the shortcuts page and the command palette.
- `matchesShortcut(s, event)` matches a real `KeyboardEvent` and rejects the wrong-platform modifier so Ctrl+K on macOS never fires a `mod: true` shortcut.
- `matchesGroupDigit(event)` returns 1-9 if Mod+digit was pressed. Group switching is dynamic (driven by `orderedGroupTargets` in `page.tsx`) and isn't in the `SHORTCUTS` table.

Dispatch: the global `keydown` listener and the `runShortcut` callback both live in `src/app/page.tsx`. To add a new static shortcut:

1. Append to `SHORTCUTS` in `src/lib/shortcuts.ts`. Add the `ShortcutId` variant.
2. Add a `case "yourId":` in `runShortcut` in `page.tsx`.
3. Add the icon mapping in `src/components/command-palette.tsx::ICONS`.
4. Add `shortcuts.yourId` (label) to every locale file in `src/i18n/locales/`.

The command palette (Mod+K) is built on the shadcn `Command` primitive with a token-AND fuzzy filter (`fuzzyFilter` in `command-palette.tsx`). The `CommandDialog` wrapper now forwards `filter`/`shouldFilter` to the inner `Command` for callers that need custom matching.

## Singletons

- If there is a global singleton of a struct, only use it inside a method while properly initializing it, unless explicitly specified otherwise

## UI Theming

The fork has its own look, Owl Graphite: blue-black graphite surfaces, one violet
(`--primary`) for everything that acts, mint (`--success`) for what is running. It lives in the
`.dark` block of `src/styles/globals.css` (the light counterpart is `:root`) and, with the same
values, as the first entry of `THEMES`, so `pnpm test:themes` holds it to the same contrast
rules as every preset. The faces are Onest and JetBrains Mono, both loaded with Cyrillic in
`src/app/layout.tsx`; anything marked `tabular-nums` is set in the monospace face. The
navigation (`rail-nav.tsx`) is an icon rail below 1024px and a sidebar that names every
destination above it, and the window opens at 1200x760.

- Never use hardcoded Tailwind color classes (e.g., `text-red-500`, `bg-green-600`, `border-yellow-400`). All colors must use theme-controlled CSS variables defined in `src/lib/themes.ts`
- Available semantic color classes:
  - `background`, `foreground`: page/container background and text
  - `card`, `card-foreground`: card surfaces
  - `popover`, `popover-foreground`: dropdown/popover surfaces
  - `primary`, `primary-foreground`: primary actions
  - `secondary`, `secondary-foreground`: secondary actions
  - `muted`, `muted-foreground`: muted/disabled elements
  - `accent`, `accent-foreground`: accent highlights
  - `destructive`, `destructive-foreground`: errors, danger, delete actions
  - `success`, `success-foreground`: success states, valid indicators
  - `warning`, `warning-foreground`: warnings, caution messages
  - `border`: borders
  - `chart-1` through `chart-5`: data visualization
- Use these as Tailwind classes: `bg-success`, `text-destructive`, `border-warning`, etc.
- For lighter variants use opacity: `bg-destructive/10`, `bg-success/10`, `border-warning/50`
- Quiet, borderless fills (soft buttons, fields, chips, hover and selected rows) are tints of the theme's own `foreground` (`bg-foreground/5`) or `primary` (`bg-primary/10`), never `muted`, `accent` or `secondary`: several custom themes make those loud (Tokyo Night's accent is purple, its secondary cyan). Reach for the `soft`/`subtle` Button, `soft` Badge, `soft`/`bare` Input and `soft` SelectTrigger variants, or wrap a form section in `SoftFields` (`ui/field-variant.tsx`).
- Data tables use `.data-table` (globals.css) with `DATA_TABLE_CLASSES` and `SortableColumnHeader` from `table-controls.tsx`: no row rules, one tinted header strip, rounded row highlight. Secondary row controls take `row-reveal` to appear on row hover or focus.

## App data directory naming

`src-tauri/src/app_dirs.rs::app_name()` returns `"AntidetectDev"` when `cfg!(debug_assertions)` is true, `"Antidetect"` otherwise. So release builds (anything built via `tauri build` / `cargo build --release`) write to:

- macOS: `~/Library/Application Support/Antidetect/`
- Linux: `~/.local/share/Antidetect/`
- Windows: `%LOCALAPPDATA%\Antidetect\`

Debug builds (`cargo build`, `pnpm tauri dev`) write to the `AntidetectDev` sibling at the same root, and a `dev-{version}` `BUILD_VERSION` is injected via `build.rs`. Logs and screenshots referencing `AntidetectDev` therefore mean a local dev build is in play, not a release; useful when a bug report seems to disagree with what production users see.

## Sync (cloud / self-hosted)

Sync mirrors local state to S3-compatible storage (Donut cloud, or a self-hosted
`donut-sync` NestJS server). Two distinct mechanisms live in `src-tauri/src/sync/`:

- Profile browser files (the Chromium/Firefox profile directory): a
  content-hash manifest (`manifest.rs` `generate_manifest`/`compute_diff`) does a
  per-file hash+size diff, so only changed files transfer. `sync_profile` in
  `engine.rs`.
- Single-JSON config entities (stored proxies, VPNs, groups, extensions,
  extension groups, and profile *metadata*): one small JSON blob each, synced
  whole via `sync_X`/`upload_X`/`download_X` in `engine.rs`.

### Conflict resolution: one rule everywhere, `updated_at` last-write-wins

Every config entity carries `updated_at: Option<u64>` (unix seconds;
`extension_manager` uses a non-Optional `u64`). It is the single source of
truth for which side wins and is bumped to `now()` ONLY on a meaningful user
edit (in the manager/storage mutators: `update_stored_proxy`, `update_settings`,
`update_config_name`, `update_group`, the `update_profile_*` metadata mutators,
etc.), NEVER by sync bookkeeping. Use `crate::proxy_manager::now_secs()`.

`last_sync` is display/bookkeeping only ("last synced at"). It is written on
every upload/download and must NOT decide sync direction. (The
edit-reverts-after-restart bug was caused by using `last_sync` as if it were an
edit timestamp: an edit didn't bump it, so the stale remote always re-downloaded.)

Reconcile (`engine.rs::remote_updated_at` + each `sync_X`):
1. `stat` (HEAD) the remote object. Its `updated_at` is read from S3 object
   metadata (`x-amz-meta-updated-at`), with no body download when nothing changed.
2. Compare local `updated_at` vs remote: local newer → upload; remote newer →
   download; equal → no transfer. Legacy objects with no timestamp resolve to 0,
   so any real edit wins.
3. Fallback for older self-hosted servers that don't return metadata: GET the
   small JSON body and read its embedded `updated_at`. Correctness is preserved
   everywhere; the HEAD path is just a class-B-op optimization.

Uploads go through `engine.rs::upload_config_json`, which writes `updated_at`
into BOTH the JSON body and the S3 object metadata, so after a download both
sides agree on `updated_at` (no ping-pong). Adding a new synced config field?
Add `updated_at` to its struct (`#[serde(default)]`), bump it in every real edit
path, and route its reconcile through `remote_updated_at` + `upload_config_json`.

### Server (`donut-sync/`) metadata passthrough

`presignUpload` signs request `metadata` into the PUT as `x-amz-meta-*` and
echoes back what it signed (the Rust client must send exactly those headers on
the PUT or S3 rejects it, hence the echo). `stat` returns `response.Metadata`.
Older servers omit `metadata` → client falls back to the body-GET path. DTOs:
`donut-sync/src/sync/dto/sync.dto.ts`; logic: `sync.service.ts`.

## License

AGPL-3.0, inherited from upstream. This fork is used internally; keep the license file and
upstream notices intact.
