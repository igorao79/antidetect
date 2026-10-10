//! Cookie Robot: walks a list of sites in a set of profiles so they collect
//! cookies and history before they are put to work.
//!
//! Everything happens on this machine. Each profile is launched the way an
//! automation client launches it, driven over its debugging endpoint (CDP for
//! the Chromium engines, WebDriver BiDi for Camoufox) and closed again, so the
//! browser writes what it collected. One run at a time; its progress is kept
//! here and sent to the UI as `cookie-robot-progress`.

use crate::browser_runner::{BrowserRunner, LaunchOptions};
use crate::profile::BrowserProfile;
use futures_util::{SinkExt, StreamExt};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

const MAX_SITES: usize = 200;
const MAX_PARALLEL: usize = 5;
const SECONDS_RANGE: std::ops::RangeInclusive<u64> = 3..=600;
/// How long a freshly launched browser gets to open its debugging endpoint.
const ENDPOINT_TIMEOUT: Duration = Duration::from_secs(40);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// A page still loading after this is left as it is and scrolled anyway.
const LOAD_TIMEOUT: Duration = Duration::from_secs(40);

const PROGRESS_EVENT: &str = "cookie-robot-progress";

fn default_min_seconds() -> u64 {
  8
}

fn default_max_seconds() -> u64 {
  20
}

fn default_parallel() -> usize {
  1
}

#[derive(Debug, Clone, Deserialize)]
pub struct CookieRobotRequest {
  #[serde(default)]
  pub profile_ids: Vec<String>,
  /// One site per entry; a bare host is visited over HTTPS.
  pub urls: Vec<String>,
  /// Time spent on each site, drawn between the two for every visit.
  #[serde(default = "default_min_seconds")]
  pub min_seconds: u64,
  #[serde(default = "default_max_seconds")]
  pub max_seconds: u64,
  /// Visit the sites in a different order in every profile.
  #[serde(default)]
  pub shuffle: bool,
  #[serde(default)]
  pub headless: bool,
  /// Profiles warmed at the same time.
  #[serde(default = "default_parallel")]
  pub parallel: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarmState {
  Queued,
  Running,
  Done,
  Failed,
  Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileProgress {
  pub profile_id: String,
  pub name: String,
  pub state: WarmState,
  /// Sites that loaded.
  pub visited: usize,
  /// Sites that could not be opened.
  pub unreachable: usize,
  pub total: usize,
  pub current_url: Option<String>,
  /// A backend error code, for a profile that could not be warmed at all.
  pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CookieRobotStatus {
  pub running: bool,
  pub profiles: Vec<ProfileProgress>,
}

struct Run {
  status: CookieRobotStatus,
  cancel: CancellationToken,
}

static RUN: LazyLock<Mutex<Option<Run>>> = LazyLock::new(|| Mutex::new(None));

/// What a validated request asks for.
struct Plan {
  urls: Vec<String>,
  min_seconds: u64,
  max_seconds: u64,
  shuffle: bool,
  headless: bool,
}

fn coded(code: &str, params: Value) -> String {
  json!({ "code": code, "params": params }).to_string()
}

/// The sites of a request as full URLs: blank lines dropped, `https://` put in
/// front of bare hosts, duplicates removed.
fn normalize_urls(urls: &[String]) -> Result<Vec<String>, String> {
  let mut sites = Vec::new();
  for raw in urls {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
      continue;
    }
    let candidate = if trimmed.contains("://") {
      trimmed.to_string()
    } else {
      format!("https://{trimmed}")
    };
    let valid = url::Url::parse(&candidate)
      .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if !valid {
      return Err(coded("COOKIE_ROBOT_INVALID_URL", json!({ "url": trimmed })));
    }
    if !sites.contains(&candidate) {
      sites.push(candidate);
    }
  }
  if sites.is_empty() {
    return Err(crate::backend_error("COOKIE_ROBOT_NO_SITES"));
  }
  if sites.len() > MAX_SITES {
    return Err(coded(
      "COOKIE_ROBOT_TOO_MANY_SITES",
      json!({ "max": MAX_SITES.to_string() }),
    ));
  }
  Ok(sites)
}

fn plan_for(request: &CookieRobotRequest) -> Result<Plan, String> {
  let min_seconds = request
    .min_seconds
    .clamp(*SECONDS_RANGE.start(), *SECONDS_RANGE.end());
  let max_seconds = request.max_seconds.clamp(min_seconds, *SECONDS_RANGE.end());
  Ok(Plan {
    urls: normalize_urls(&request.urls)?,
    min_seconds,
    max_seconds,
    shuffle: request.shuffle,
    headless: request.headless,
  })
}

pub fn status() -> CookieRobotStatus {
  RUN
    .lock()
    .unwrap()
    .as_ref()
    .map(|run| run.status.clone())
    .unwrap_or_default()
}

fn update(profile_id: &str, change: impl FnOnce(&mut ProfileProgress)) {
  let snapshot = {
    let mut guard = RUN.lock().unwrap();
    let Some(run) = guard.as_mut() else {
      return;
    };
    if let Some(progress) = run
      .status
      .profiles
      .iter_mut()
      .find(|p| p.profile_id == profile_id)
    {
      change(progress);
    }
    run.status.clone()
  };
  let _ = crate::events::emit(PROGRESS_EVENT, &snapshot);
}

/// Start warming `request.profile_ids`. Returns at once with the queued run;
/// progress follows as events and through [`status`].
pub fn start(
  app_handle: tauri::AppHandle,
  request: CookieRobotRequest,
) -> Result<CookieRobotStatus, String> {
  let plan = plan_for(&request)?;
  let all = crate::profile::ProfileManager::instance()
    .list_profiles()
    .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?;
  let mut profiles: Vec<BrowserProfile> = Vec::new();
  for id in &request.profile_ids {
    if profiles.iter().any(|p| &p.id.to_string() == id) {
      continue;
    }
    let profile = all
      .iter()
      .find(|p| &p.id.to_string() == id)
      .ok_or_else(|| crate::backend_error("PROFILE_NOT_FOUND"))?;
    profiles.push(profile.clone());
  }
  if profiles.is_empty() {
    return Err(crate::backend_error("COOKIE_ROBOT_NO_PROFILES"));
  }

  let cancel = CancellationToken::new();
  let initial = {
    let mut guard = RUN.lock().unwrap();
    if guard.as_ref().is_some_and(|run| run.status.running) {
      return Err(crate::backend_error("COOKIE_ROBOT_ALREADY_RUNNING"));
    }
    let status = CookieRobotStatus {
      running: true,
      profiles: profiles
        .iter()
        .map(|profile| ProfileProgress {
          profile_id: profile.id.to_string(),
          name: profile.name.clone(),
          state: WarmState::Queued,
          visited: 0,
          unreachable: 0,
          total: plan.urls.len(),
          current_url: None,
          error: None,
        })
        .collect(),
    };
    *guard = Some(Run {
      status: status.clone(),
      cancel: cancel.clone(),
    });
    status
  };
  let _ = crate::events::emit(PROGRESS_EVENT, &initial);

  let parallel = request.parallel.clamp(1, MAX_PARALLEL);
  tauri::async_runtime::spawn(run(app_handle, profiles, Arc::new(plan), parallel, cancel));
  Ok(initial)
}

/// Stop the current run: profiles being warmed are closed, the rest are left
/// untouched. Returns whether there was a run to stop.
pub fn stop() -> bool {
  let guard = RUN.lock().unwrap();
  match guard.as_ref() {
    Some(run) if run.status.running => {
      run.cancel.cancel();
      true
    }
    _ => false,
  }
}

async fn run(
  app_handle: tauri::AppHandle,
  profiles: Vec<BrowserProfile>,
  plan: Arc<Plan>,
  parallel: usize,
  cancel: CancellationToken,
) {
  let slots = Arc::new(tokio::sync::Semaphore::new(parallel));
  let mut tasks = tokio::task::JoinSet::new();
  for profile in profiles {
    let (app_handle, plan, cancel, slots) = (
      app_handle.clone(),
      plan.clone(),
      cancel.clone(),
      slots.clone(),
    );
    tasks.spawn(async move {
      let _slot = slots.acquire_owned().await;
      let id = profile.id.to_string();
      if cancel.is_cancelled() {
        update(&id, |p| p.state = WarmState::Cancelled);
        return;
      }
      update(&id, |p| p.state = WarmState::Running);
      let outcome = warm_profile(app_handle, &profile, &plan, &cancel).await;
      update(&id, |p| {
        p.current_url = None;
        match outcome {
          Ok(()) if cancel.is_cancelled() => p.state = WarmState::Cancelled,
          Ok(()) => p.state = WarmState::Done,
          Err(error) => {
            p.state = WarmState::Failed;
            p.error = Some(error);
          }
        }
      });
    });
  }
  while tasks.join_next().await.is_some() {}

  let finished = {
    let mut guard = RUN.lock().unwrap();
    guard.as_mut().map(|run| {
      run.status.running = false;
      run.status.clone()
    })
  };
  if let Some(finished) = finished {
    let _ = crate::events::emit(PROGRESS_EVENT, &finished);
  }
}

async fn free_port() -> Result<u16, String> {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
    .await
    .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?;
  listener
    .local_addr()
    .map(|addr| addr.port())
    .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))
}

async fn warm_profile(
  app_handle: tauri::AppHandle,
  profile: &BrowserProfile,
  plan: &Plan,
  cancel: &CancellationToken,
) -> Result<(), String> {
  let id = profile.id.to_string();
  if crate::browser_runner::browser_is_running_for(&id) {
    return Err(crate::backend_error("COOKIE_ROBOT_PROFILE_RUNNING"));
  }
  let port = free_port().await?;
  crate::team_lock::acquire_team_lock_if_needed(profile).await?;
  let launched = crate::browser_runner::launch_browser_profile_impl(
    app_handle.clone(),
    profile.clone(),
    None,
    LaunchOptions::automation(Some(port), plan.headless),
  )
  .await;
  let launched = match launched {
    Ok(launched) => launched,
    Err(error) => {
      crate::team_lock::release_team_lock_if_needed(profile).await;
      return Err(error);
    }
  };

  let outcome = visit_sites(&launched, port, plan, cancel).await;

  if let Err(e) = BrowserRunner::instance()
    .kill_browser_process(app_handle, &launched)
    .await
  {
    log::warn!("Cookie Robot could not close profile {}: {e}", profile.name);
  }
  crate::team_lock::release_team_lock_if_needed(profile).await;
  outcome
}

async fn visit_sites(
  profile: &BrowserProfile,
  port: u16,
  plan: &Plan,
  cancel: &CancellationToken,
) -> Result<(), String> {
  let id = profile.id.to_string();
  let mut page = Page::connect(&profile.browser, port)
    .await
    .map_err(|e| crate::backend_error_with_detail("COOKIE_ROBOT_NO_CONTROL", e))?;

  let mut order = plan.urls.clone();
  if plan.shuffle {
    use rand::seq::SliceRandom;
    order.shuffle(&mut rand::rng());
  }

  for url in &order {
    if cancel.is_cancelled() {
      break;
    }
    update(&id, |p| p.current_url = Some(url.clone()));
    match page.navigate(url).await {
      Ok(()) => {
        let stay = Duration::from_secs(between(plan.min_seconds, plan.max_seconds));
        page.linger(stay, cancel).await;
        update(&id, |p| p.visited += 1);
      }
      Err(e) => {
        log::info!("Cookie Robot: {url} did not open in {}: {e}", profile.name);
        update(&id, |p| p.unreachable += 1);
      }
    }
  }
  page.close().await;
  Ok(())
}

fn between(low: u64, high: u64) -> u64 {
  rand::rng().random_range(low..=high.max(low))
}

type Socket =
  tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

enum Protocol {
  /// Chrome DevTools Protocol, attached to one page target.
  Cdp,
  /// WebDriver BiDi, with the browsing context the session drives.
  Bidi { context: String },
}

/// The one tab the robot drives in a launched browser.
struct Page {
  socket: Socket,
  protocol: Protocol,
  next_id: u64,
}

fn local_client() -> Result<reqwest::Client, String> {
  reqwest::Client::builder()
    .no_proxy()
    .timeout(Duration::from_secs(3))
    .build()
    .map_err(|e| e.to_string())
}

/// The DevTools socket of the browser's first page target.
async fn cdp_page_socket(port: u16) -> Result<String, String> {
  let targets: Value = local_client()?
    .get(format!("http://127.0.0.1:{port}/json"))
    .send()
    .await
    .map_err(|e| e.to_string())?
    .json()
    .await
    .map_err(|e| e.to_string())?;
  targets
    .as_array()
    .and_then(|targets| targets.iter().find(|t| t["type"] == "page"))
    .and_then(|target| target["webSocketDebuggerUrl"].as_str())
    .map(str::to_string)
    .ok_or_else(|| "the browser has no page to drive".to_string())
}

impl Page {
  /// Attach to the browser listening on `port`, waiting for it to come up.
  async fn connect(browser: &str, port: u16) -> Result<Self, String> {
    let bidi = browser == "camoufox";
    let deadline = Instant::now() + ENDPOINT_TIMEOUT;
    let mut last_error = String::new();
    while Instant::now() < deadline {
      let endpoint = if bidi {
        Ok(format!("ws://127.0.0.1:{port}/session"))
      } else {
        cdp_page_socket(port).await
      };
      let attempt = match endpoint {
        Ok(endpoint) => tokio_tungstenite::connect_async(endpoint)
          .await
          .map_err(|e| e.to_string()),
        Err(e) => Err(e),
      };
      match attempt {
        Ok((socket, _)) => {
          let mut page = Page {
            socket,
            protocol: Protocol::Cdp,
            next_id: 0,
          };
          if bidi {
            page
              .call(
                "session.new",
                json!({ "capabilities": {} }),
                COMMAND_TIMEOUT,
              )
              .await?;
            let tree = page
              .call("browsingContext.getTree", json!({}), COMMAND_TIMEOUT)
              .await?;
            let context = tree["contexts"][0]["context"]
              .as_str()
              .ok_or("the browser has no tab to drive")?
              .to_string();
            page.protocol = Protocol::Bidi { context };
          } else {
            page.call("Page.enable", json!({}), COMMAND_TIMEOUT).await?;
          }
          return Ok(page);
        }
        Err(e) => last_error = e,
      }
      tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
      "the browser's debugging endpoint did not open: {last_error}"
    ))
  }

  /// Send one command and wait for its answer, skipping the events and stale
  /// answers that arrive in between.
  async fn call(
    &mut self,
    method: &str,
    params: Value,
    timeout: Duration,
  ) -> Result<Value, String> {
    self.next_id += 1;
    let id = self.next_id;
    let command = json!({ "id": id, "method": method, "params": params });
    self
      .socket
      .send(Message::text(command.to_string()))
      .await
      .map_err(|e| format!("{method}: {e}"))?;

    let deadline = Instant::now() + timeout;
    loop {
      let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| format!("{method} timed out"))?;
      let frame = tokio::time::timeout(remaining, self.socket.next())
        .await
        .map_err(|_| format!("{method} timed out"))?
        .ok_or_else(|| format!("{method}: the browser closed the connection"))?
        .map_err(|e| format!("{method}: {e}"))?;
      let Message::Text(text) = frame else {
        continue;
      };
      let Ok(message) = serde_json::from_str::<Value>(text.as_ref()) else {
        continue;
      };
      if message["id"].as_u64() != Some(id) {
        continue;
      }
      // CDP reports a failure as an `error` object, BiDi as an `error` string
      // beside a `message`.
      if let Some(error) = message.get("error") {
        let detail = error["message"]
          .as_str()
          .or_else(|| message["message"].as_str())
          .or_else(|| error.as_str())
          .unwrap_or("unknown error");
        return Err(format!("{method}: {detail}"));
      }
      return Ok(message["result"].clone());
    }
  }

  async fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
    match &self.protocol {
      Protocol::Cdp => {
        let params = json!({ "expression": expression, "returnByValue": true });
        let result = self
          .call("Runtime.evaluate", params, COMMAND_TIMEOUT)
          .await?;
        Ok(result["result"]["value"].clone())
      }
      Protocol::Bidi { context } => {
        let params = json!({
          "expression": expression,
          "target": { "context": context },
          "awaitPromise": false,
        });
        let result = self
          .call("script.evaluate", params, COMMAND_TIMEOUT)
          .await?;
        Ok(result["result"]["value"].clone())
      }
    }
  }

  /// Open `url` and wait for it to load. A page that is still loading at the
  /// deadline counts as opened; one the browser refuses to open does not.
  async fn navigate(&mut self, url: &str) -> Result<(), String> {
    match &self.protocol {
      Protocol::Cdp => {
        let result = self
          .call("Page.navigate", json!({ "url": url }), COMMAND_TIMEOUT)
          .await?;
        if let Some(error) = result["errorText"].as_str().filter(|e| !e.is_empty()) {
          return Err(error.to_string());
        }
        let deadline = Instant::now() + LOAD_TIMEOUT;
        while Instant::now() < deadline {
          tokio::time::sleep(Duration::from_millis(500)).await;
          if self.evaluate("document.readyState").await.ok() == Some(json!("complete")) {
            break;
          }
        }
        Ok(())
      }
      Protocol::Bidi { context } => {
        let params = json!({ "context": context, "url": url, "wait": "complete" });
        match self
          .call("browsingContext.navigate", params, LOAD_TIMEOUT)
          .await
        {
          Err(e) if e.ends_with("timed out") => Ok(()),
          other => other.map(|_| ()),
        }
      }
    }
  }

  /// Stay on the page for `stay`, scrolling the way a reader does: mostly
  /// down, in uneven steps, now and then back up.
  async fn linger(&mut self, stay: Duration, cancel: &CancellationToken) {
    let until = Instant::now() + stay;
    while let Some(remaining) = until.checked_duration_since(Instant::now()) {
      let pause = Duration::from_millis(between(1200, 4000)).min(remaining);
      tokio::select! {
        _ = cancel.cancelled() => return,
        _ = tokio::time::sleep(pause) => {}
      }
      let step = between(200, 800) as i64;
      let step = if between(0, 9) < 2 { -step / 2 } else { step };
      let scroll = format!("window.scrollBy({{ top: {step}, behavior: 'smooth' }})");
      if self.evaluate(&scroll).await.is_err() {
        return;
      }
    }
  }

  async fn close(mut self) {
    if matches!(self.protocol, Protocol::Bidi { .. }) {
      let _ = self
        .call("session.end", json!({}), Duration::from_secs(3))
        .await;
    }
    let _ = self.socket.close(None).await;
  }
}

#[tauri::command]
pub async fn start_cookie_robot(
  app_handle: tauri::AppHandle,
  request: CookieRobotRequest,
) -> Result<CookieRobotStatus, String> {
  start(app_handle, request)
}

#[tauri::command]
pub fn stop_cookie_robot() -> bool {
  stop()
}

#[tauri::command]
pub fn get_cookie_robot_status() -> CookieRobotStatus {
  status()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn request(urls: &[&str]) -> CookieRobotRequest {
    CookieRobotRequest {
      profile_ids: vec!["a".to_string()],
      urls: urls.iter().map(|u| u.to_string()).collect(),
      min_seconds: default_min_seconds(),
      max_seconds: default_max_seconds(),
      shuffle: false,
      headless: true,
      parallel: 1,
    }
  }

  fn code_of(error: &str) -> String {
    serde_json::from_str::<Value>(error).unwrap()["code"]
      .as_str()
      .unwrap()
      .to_string()
  }

  #[test]
  fn sites_are_cleaned_up_into_full_urls() {
    let sites = normalize_urls(&[
      "  example.com ".to_string(),
      String::new(),
      "http://plain.example/path?q=1".to_string(),
      "https://example.com".to_string(),
    ])
    .unwrap();
    assert_eq!(
      sites,
      ["https://example.com", "http://plain.example/path?q=1"]
    );
  }

  #[test]
  fn a_site_that_is_not_a_web_address_is_named_in_the_error() {
    for bad in ["file:///etc/passwd", "javascript:alert(1)", "https://"] {
      let error = normalize_urls(&[bad.to_string()]).unwrap_err();
      assert_eq!(code_of(&error), "COOKIE_ROBOT_INVALID_URL", "{bad}");
      assert!(error.contains(bad), "{error}");
    }
  }

  #[test]
  fn a_run_needs_at_least_one_site_and_not_too_many() {
    let none = normalize_urls(&["  ".to_string()]).unwrap_err();
    assert_eq!(code_of(&none), "COOKIE_ROBOT_NO_SITES");

    let many: Vec<String> = (0..=MAX_SITES)
      .map(|n| format!("site{n}.example"))
      .collect();
    assert_eq!(
      code_of(&normalize_urls(&many).unwrap_err()),
      "COOKIE_ROBOT_TOO_MANY_SITES"
    );
  }

  #[test]
  fn time_on_a_site_is_kept_in_range_and_in_order() {
    let mut odd = request(&["example.com"]);
    odd.min_seconds = 900;
    odd.max_seconds = 1;
    let plan = plan_for(&odd).unwrap();
    assert_eq!((plan.min_seconds, plan.max_seconds), (600, 600));

    odd.min_seconds = 0;
    odd.max_seconds = 5;
    let plan = plan_for(&odd).unwrap();
    assert_eq!((plan.min_seconds, plan.max_seconds), (3, 5));
  }
}
