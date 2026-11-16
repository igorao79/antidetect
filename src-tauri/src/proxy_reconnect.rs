//! Brings a session back after the kill switch cut its worker (fork).
//!
//! With "Block launches without a proxy or VPN" on, a route that goes Down
//! has its local worker stopped (`proxy_manager::stop_worker_for_down_route`):
//! the browser runs on, but its proxy port answers nothing, so no request
//! leaves the machine. Upstream left it there until a restart. Here the cut
//! session waits for its proxy: every attempt starts a throwaway worker on
//! another port and measures the exit through it. Only when that exit is in
//! the place the browser was launched for (the time zone and country its
//! fingerprint carries) is the session's worker started again, on the port
//! the browser points at, through the same upstream
//! (`ProxyManager::reconnect_session`, which re-checks the session under the
//! worker lifecycle lock). Nothing listens on that port before then, so the
//! browser has no way out in between. A rotating upstream can still hand the
//! restarted worker another exit than the probe saw; the route health check
//! watches it from there as it watches any session.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::browser::ProxySettings;
use crate::events;
use crate::proxy_manager::PROXY_MANAGER;

/// Pauses between attempts: soon after the cut, then once a minute.
const RETRY_DELAYS_SECS: [u64; 3] = [15, 30, 60];
/// How long one exit lookup through the probe worker may take.
const EXIT_LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);

/// What a worker start of a profile was given, so the same worker can be
/// started again. Kept in memory only: it holds the proxy's credentials.
#[derive(Clone, Debug)]
pub struct WorkerStart {
  pub settings: ProxySettings,
  pub bypass_rules: Vec<String>,
  pub blocklist_file: Option<String>,
  pub dns_allowlist_mode: bool,
  pub dns_server: Option<String>,
  pub record_domains: bool,
  pub local_protocol: String,
}

/// A session the kill switch cut, as its worker had it.
#[derive(Clone, Debug)]
pub struct CutSession {
  pub browser_pid: u32,
  pub local_port: u16,
  /// Type, host and port of the upstream the worker dialed.
  pub upstream: (String, String, u16),
}

/// What `ProxyManager::reconnect_session` did.
pub enum Restart {
  Started,
  /// Try again later.
  Retry(String),
  /// The session is not the cut one any more.
  Abandon(&'static str),
}

/// Where the browser behind a local port was told it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchExit {
  /// The fingerprint took the exit's place at launch.
  Follows(ExitPlace),
  /// The fingerprint does not follow the exit (automatic location off, or a
  /// fixed address): the person chose its place, so any exit of the same
  /// proxy reconnects.
  Fixed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitPlace {
  pub timezone: String,
  pub country: Option<String>,
}

impl ExitPlace {
  pub fn of(geo: &crate::geolocation::Geolocation) -> Self {
    Self {
      timezone: geo.timezone.clone(),
      country: geo.locale.region.as_ref().map(|r| r.to_uppercase()),
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
enum ExitCheck {
  Same,
  Moved,
  /// The launch left no record, or the new exit could not be placed.
  Unknown,
}

fn compare_exit(launch: Option<&LaunchExit>, now: Option<&ExitPlace>) -> ExitCheck {
  match (launch, now) {
    (Some(LaunchExit::Fixed), _) => ExitCheck::Same,
    (Some(LaunchExit::Follows(then)), Some(now)) if then == now => ExitCheck::Same,
    (Some(LaunchExit::Follows(_)), Some(_)) => ExitCheck::Moved,
    _ => ExitCheck::Unknown,
  }
}

fn retry_delay(attempt: usize) -> Duration {
  let index = attempt.min(RETRY_DELAYS_SECS.len() - 1);
  Duration::from_secs(RETRY_DELAYS_SECS[index])
}

fn same_upstream(start: &WorkerStart, session: &CutSession) -> bool {
  let (kind, host, port) = &session.upstream;
  start.settings.proxy_type == *kind && start.settings.host == *host && start.settings.port == *port
}

/// The latest start of each profile, with a generation that changes at every
/// start, so a reconnect can tell its session from a newer one.
static STARTS: LazyLock<Mutex<HashMap<String, (u64, WorkerStart)>>> =
  LazyLock::new(Default::default);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static LAUNCH_EXITS: LazyLock<Mutex<HashMap<u16, LaunchExit>>> = LazyLock::new(Default::default);
static WAITING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Record a worker start of a profile. A fresh start (a launch, a proxy
/// change) belongs to a new browser session, so what an earlier session on
/// the same port was told no longer holds; a reconnect keeps it.
pub fn remember_start(profile_id: &str, local_port: u16, reconnect: bool, start: WorkerStart) {
  let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
  if let Ok(mut starts) = STARTS.lock() {
    starts.insert(profile_id.to_string(), (generation, start));
  }
  if !reconnect {
    if let Ok(mut exits) = LAUNCH_EXITS.lock() {
      exits.remove(&local_port);
    }
  }
}

/// Forget a profile's session: it stopped, or started without an upstream.
pub fn forget(profile_id: &str, local_port: Option<u16>) {
  if let Ok(mut starts) = STARTS.lock() {
    starts.remove(profile_id);
  }
  if let Some(port) = local_port {
    if let Ok(mut exits) = LAUNCH_EXITS.lock() {
      exits.remove(&port);
    }
  }
}

pub fn start_is_current(profile_id: &str, generation: u64) -> bool {
  STARTS
    .lock()
    .ok()
    .and_then(|starts| starts.get(profile_id).map(|(g, _)| *g == generation))
    .unwrap_or(false)
}

fn current_start(profile_id: &str) -> Option<(u64, WorkerStart)> {
  STARTS.lock().ok()?.get(profile_id).cloned()
}

/// Where a Camoufox fingerprint puts the browser, as its pages read it.
/// `None` when a fingerprint that follows the exit names no time zone.
pub fn launch_exit_of_fingerprint(
  follows_exit: bool,
  fingerprint: Option<&str>,
) -> Option<LaunchExit> {
  if !follows_exit {
    return Some(LaunchExit::Fixed);
  }
  let fingerprint: HashMap<String, serde_json::Value> = serde_json::from_str(fingerprint?).ok()?;
  let timezone = fingerprint.get("timezone")?.as_str()?.to_string();
  let country = fingerprint
    .get("locale:region")
    .and_then(|region| region.as_str())
    .map(str::to_uppercase);
  Some(LaunchExit::Follows(ExitPlace { timezone, country }))
}

/// Record where the browser launched behind `local_url` was told it is.
pub fn note_launch_exit(local_url: Option<&str>, exit: LaunchExit) {
  let Some(port) = local_url
    .and_then(|url| url::Url::parse(url).ok())
    .and_then(|url| url.port())
  else {
    return;
  };
  if let Ok(mut exits) = LAUNCH_EXITS.lock() {
    exits.insert(port, exit);
  }
}

fn emit(profile_id: &str, payload: serde_json::Value) {
  let mut payload = payload;
  payload["profile_id"] = serde_json::Value::from(profile_id);
  if let Err(e) = events::emit("proxy-health-changed", payload) {
    log::warn!("Event emit failed event=proxy-health-changed err=\"{e}\"");
  }
}

/// Start waiting for the proxy of a session the kill switch just cut.
pub fn begin(app_handle: tauri::AppHandle, profile_id: &str, session: CutSession) {
  let Some((generation, start)) = current_start(profile_id) else {
    return;
  };
  // Without a record of where the browser was launched for (Wayfern, an exit
  // that could not be placed), or when the recorded start is not the one the
  // cut worker ran (its route changed since), a new exit cannot be checked:
  // the session stays cut and the banner asks for a restart, as before.
  let has_launch_exit = LAUNCH_EXITS
    .lock()
    .map(|exits| exits.contains_key(&session.local_port))
    .unwrap_or(false);
  if !has_launch_exit || !same_upstream(&start, &session) {
    return;
  }
  let first = WAITING
    .lock()
    .map(|mut waiting| waiting.insert(profile_id.to_string()))
    .unwrap_or(false);
  if !first {
    return;
  }
  emit(profile_id, serde_json::json!({ "reconnecting": true }));
  let profile_id = profile_id.to_string();
  tauri::async_runtime::spawn(async move {
    let outcome = wait_and_reconnect(&app_handle, &profile_id, &session, generation, &start).await;
    if let Ok(mut waiting) = WAITING.lock() {
      waiting.remove(&profile_id);
    }
    match outcome {
      Ok(()) => emit(
        &profile_id,
        serde_json::json!({ "status": "ok", "at": crate::proxy_manager::now_secs() }),
      ),
      Err(code) => emit(
        &profile_id,
        serde_json::json!({ "reconnect_stopped": code }),
      ),
    }
  });
}

/// Wait until the session is back (`Ok`) or no longer to be reconnected
/// (`Err` with why).
async fn wait_and_reconnect(
  app_handle: &tauri::AppHandle,
  profile_id: &str,
  session: &CutSession,
  generation: u64,
  start: &WorkerStart,
) -> Result<(), &'static str> {
  let started = std::time::Instant::now();
  for attempt in 0.. {
    tokio::time::sleep(retry_delay(attempt)).await;
    if !crate::proxy_storage::is_process_running(session.browser_pid) {
      log::info!(
        "Reconnect dropped, browser gone profile={profile_id} pid={}",
        session.browser_pid
      );
      return Err("SESSION_ENDED");
    }
    if !start_is_current(profile_id, generation) {
      return Err("SESSION_ENDED");
    }
    let why = match try_once(app_handle, profile_id, session, generation, start).await {
      Attempt::Reconnected => {
        log::info!(
          "Session reconnected profile={profile_id} port={} attempts={} after_s={}",
          session.local_port,
          attempt + 1,
          started.elapsed().as_secs()
        );
        return Ok(());
      }
      Attempt::Stop(code) => {
        log::warn!("Reconnect stopped profile={profile_id} code={code}");
        return Err(code);
      }
      Attempt::NotYet(why) => why,
    };
    if attempt == 0 {
      log::info!("Reconnect waiting for the proxy profile={profile_id} why=\"{why}\"");
    } else {
      log::debug!("Reconnect attempt failed profile={profile_id} why=\"{why}\"");
    }
  }
  Err("SESSION_ENDED")
}

enum Attempt {
  Reconnected,
  NotYet(String),
  Stop(&'static str),
}

async fn try_once(
  app_handle: &tauri::AppHandle,
  profile_id: &str,
  session: &CutSession,
  generation: u64,
  start: &WorkerStart,
) -> Attempt {
  let exit = match probe_exit(app_handle, start).await {
    Ok(ip) => ip,
    Err(why) => return Attempt::NotYet(why),
  };
  let now = crate::geolocation::get_geolocation(&exit)
    .ok()
    .map(|geo| ExitPlace::of(&geo));
  let launch = LAUNCH_EXITS
    .lock()
    .ok()
    .and_then(|exits| exits.get(&session.local_port).cloned());
  match compare_exit(launch.as_ref(), now.as_ref()) {
    ExitCheck::Same => {}
    ExitCheck::Moved => return Attempt::Stop("PROXY_EXIT_MOVED"),
    ExitCheck::Unknown => return Attempt::Stop("PROXY_EXIT_UNVERIFIED"),
  }
  match PROXY_MANAGER
    .reconnect_session(app_handle.clone(), profile_id, session, generation, start)
    .await
  {
    Restart::Started => Attempt::Reconnected,
    Restart::Retry(why) => Attempt::NotYet(why),
    Restart::Abandon(why) => {
      log::info!("Reconnect abandoned profile={profile_id} why={why}");
      Attempt::Stop("SESSION_ENDED")
    }
  }
}

/// The exit the proxy gives now, measured through a throwaway worker on a
/// port of its own, so the browser's port stays silent until the answer is
/// known. Works for every proxy type the worker dials (SSH, share links).
async fn probe_exit(app_handle: &tauri::AppHandle, start: &WorkerStart) -> Result<String, String> {
  let probe_pid = crate::proxy_manager::next_launch_placeholder_pid();
  let local = PROXY_MANAGER
    .start_proxy(
      app_handle.clone(),
      Some(&start.settings),
      probe_pid,
      None,
      start.bypass_rules.clone(),
      None,
      false,
      start.dns_server.as_deref(),
      false,
      &start.local_protocol,
    )
    .await?;
  let scheme = if local.proxy_type.starts_with("socks") {
    "socks5h"
  } else {
    "http"
  };
  let url = format!("{scheme}://127.0.0.1:{}", local.port);
  let ip = tokio::time::timeout(
    EXIT_LOOKUP_TIMEOUT,
    crate::ip_utils::fetch_public_ip(Some(&url)),
  )
  .await;
  if let Err(e) = PROXY_MANAGER
    .stop_reconnect_probe(app_handle.clone(), probe_pid)
    .await
  {
    log::warn!("Reconnect probe stop failed err=\"{e}\"");
  }
  match ip {
    Ok(Ok(ip)) => Ok(ip),
    Ok(Err(e)) => Err(e.to_string()),
    Err(_) => Err("exit lookup timed out".to_string()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn place(timezone: &str, country: &str) -> ExitPlace {
    ExitPlace {
      timezone: timezone.to_string(),
      country: Some(country.to_string()),
    }
  }

  #[test]
  fn an_exit_in_the_launch_place_reconnects() {
    let launch = LaunchExit::Follows(place("America/Mexico_City", "MX"));
    let now = place("America/Mexico_City", "MX");
    assert_eq!(compare_exit(Some(&launch), Some(&now)), ExitCheck::Same);
  }

  #[test]
  fn an_exit_in_another_time_zone_or_country_does_not() {
    let launch = LaunchExit::Follows(place("America/Mexico_City", "MX"));
    assert_eq!(
      compare_exit(Some(&launch), Some(&place("America/Tijuana", "MX"))),
      ExitCheck::Moved
    );
    assert_eq!(
      compare_exit(Some(&launch), Some(&place("America/Mexico_City", "US"))),
      ExitCheck::Moved
    );
  }

  #[test]
  fn a_fixed_location_reconnects_wherever_the_exit_is() {
    assert_eq!(
      compare_exit(
        Some(&LaunchExit::Fixed),
        Some(&place("Europe/Berlin", "DE"))
      ),
      ExitCheck::Same
    );
    assert_eq!(
      compare_exit(Some(&LaunchExit::Fixed), None),
      ExitCheck::Same
    );
  }

  #[test]
  fn without_a_launch_record_or_a_placed_exit_nothing_reconnects() {
    let launch = LaunchExit::Follows(place("America/Mexico_City", "MX"));
    assert_eq!(
      compare_exit(None, Some(&place("Europe/Berlin", "DE"))),
      ExitCheck::Unknown
    );
    assert_eq!(compare_exit(Some(&launch), None), ExitCheck::Unknown);
  }

  #[test]
  fn the_launch_place_is_read_from_the_fingerprint() {
    let fingerprint =
      r#"{"timezone":"America/Mexico_City","locale:region":"mx","navigator.platform":"MacIntel"}"#;
    assert_eq!(
      launch_exit_of_fingerprint(true, Some(fingerprint)),
      Some(LaunchExit::Follows(place("America/Mexico_City", "MX")))
    );
    assert_eq!(
      launch_exit_of_fingerprint(false, None),
      Some(LaunchExit::Fixed)
    );
    assert_eq!(launch_exit_of_fingerprint(true, Some("{}")), None);
    assert_eq!(launch_exit_of_fingerprint(true, None), None);
  }

  #[test]
  fn attempts_slow_down_to_once_a_minute() {
    assert_eq!(retry_delay(0), Duration::from_secs(15));
    assert_eq!(retry_delay(1), Duration::from_secs(30));
    assert_eq!(retry_delay(2), Duration::from_secs(60));
    assert_eq!(retry_delay(50), Duration::from_secs(60));
  }

  fn start() -> WorkerStart {
    WorkerStart {
      settings: ProxySettings {
        proxy_type: "http".to_string(),
        host: "203.0.113.7".to_string(),
        port: 8080,
        username: None,
        password: None,
        vless_uri: None,
      },
      bypass_rules: Vec::new(),
      blocklist_file: None,
      dns_allowlist_mode: false,
      dns_server: None,
      record_domains: true,
      local_protocol: "http".to_string(),
    }
  }

  fn session(host: &str) -> CutSession {
    CutSession {
      browser_pid: 4242,
      local_port: 53_139,
      upstream: ("http".to_string(), host.to_string(), 8080),
    }
  }

  #[test]
  fn only_a_cut_of_the_recorded_upstream_is_reconnected() {
    assert!(same_upstream(&start(), &session("203.0.113.7")));
    assert!(!same_upstream(&start(), &session("198.51.100.9")));
  }

  #[test]
  #[serial_test::serial]
  fn a_launch_exit_is_kept_by_a_reconnect_and_dropped_by_a_fresh_start() {
    let port = 61_234;
    note_launch_exit(Some("http://127.0.0.1:61234"), LaunchExit::Fixed);
    remember_start("profile-a", port, true, start());
    assert_eq!(
      LAUNCH_EXITS.lock().unwrap().get(&port),
      Some(&LaunchExit::Fixed)
    );
    remember_start("profile-a", port, false, start());
    assert_eq!(LAUNCH_EXITS.lock().unwrap().get(&port), None);
    forget("profile-a", None);
  }

  #[test]
  #[serial_test::serial]
  fn a_newer_start_or_a_stop_makes_a_reconnect_stale() {
    remember_start("profile-b", 61_300, false, start());
    let (generation, _) = current_start("profile-b").unwrap();
    assert!(start_is_current("profile-b", generation));
    remember_start("profile-b", 61_301, false, start());
    assert!(!start_is_current("profile-b", generation));
    let (newer, _) = current_start("profile-b").unwrap();
    note_launch_exit(Some("http://127.0.0.1:61301"), LaunchExit::Fixed);
    forget("profile-b", Some(61_301));
    assert!(!start_is_current("profile-b", newer));
    assert_eq!(LAUNCH_EXITS.lock().unwrap().get(&61_301), None);
  }
}
