//! Octo Browser compatible Local Client API.
//!
//! Serves the subset of Octo's local API that automation scripts use
//! (`/api/profiles/start`, `/stop`, `/force_stop`, `/active`) on
//! `127.0.0.1:58888`, so a script written for Octo drives Antidetect profiles
//! unchanged. Like Octo's, it takes no token: it only listens on loopback,
//! rejects requests whose `Host` is not a loopback name (DNS rebinding), and
//! only accepts JSON bodies, which a web page cannot send cross-origin
//! without a CORS preflight this server never answers.
//!
//! Profiles are addressed by their id, with or without dashes (Octo uses
//! 32-character hex ids).
//!
//! The profile, proxy and tag calls of Octo's cloud automation API
//! (`/api/v2/automation/profiles`, `/proxies`, `/tags`: list, read, create,
//! edit, delete, cookie import) are served here too, against local data, with
//! Octo's `{success, msg, data, code}` envelope.

use axum::{
  extract::{Path, Query, Request, State},
  http::{header, StatusCode},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::{get, patch, post},
  Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::profile::{BrowserProfile, ProfileManager};

pub const DEFAULT_PORT: u16 = 58888;

/// What a start through this API recorded, so `/active` can report the
/// endpoint the caller connected to.
#[derive(Clone)]
struct Session {
  headless: bool,
  debug_port: u16,
  ws_endpoint: String,
  start_time: u64,
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Session>>> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
struct ApiState {
  app_handle: tauri::AppHandle,
}

type ApiResult = Result<Response, ApiError>;

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (self.0, Json(json!({ "error": self.1 }))).into_response()
  }
}

fn bad_request(msg: impl Into<String>) -> ApiError {
  ApiError(StatusCode::BAD_REQUEST, msg.into())
}

/// `debug_port` is `true` (pick a free port), `false`/absent (pick one too:
/// automation always needs it) or a fixed port number.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum DebugPort {
  // Either value picks a free port; the flag is only parsed.
  Flag(#[allow(dead_code)] bool),
  Port(u16),
  #[default]
  Unset,
}

#[derive(Deserialize)]
struct StartRequest {
  uuid: String,
  #[serde(default)]
  headless: bool,
  #[serde(default)]
  debug_port: DebugPort,
  /// Octo passes extra Chromium switches here. Accepted for compatibility;
  /// engines are launched with their own managed switch set.
  #[serde(default)]
  flags: Vec<String>,
  #[serde(default)]
  url: Option<String>,
}

#[derive(Deserialize)]
struct UuidRequest {
  uuid: String,
}

#[derive(Serialize)]
struct ProfileState {
  uuid: String,
  state: &'static str,
  headless: bool,
  start_time: u64,
  ws_endpoint: String,
  debug_port: u16,
  browser_pid: Option<u32>,
  browser: String,
}

fn normalize_id(id: &str) -> String {
  id.trim().replace('-', "").to_ascii_lowercase()
}

fn octo_uuid(profile: &BrowserProfile) -> String {
  normalize_id(&profile.id.to_string())
}

fn find_profile(uuid: &str) -> Result<BrowserProfile, ApiError> {
  let wanted = normalize_id(uuid);
  if wanted.is_empty() {
    return Err(bad_request("uuid is required"));
  }
  ProfileManager::instance()
    .list_profiles()
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .find(|p| octo_uuid(p) == wanted)
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("profile {uuid} not found")))
}

fn now_secs() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs()
}

async fn free_port() -> Result<u16, ApiError> {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  listener
    .local_addr()
    .map(|addr| addr.port())
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// The endpoint an automation library connects to: CDP for Chromium engines,
/// WebDriver BiDi for Camoufox (Firefox).
async fn ws_endpoint(browser: &str, port: u16) -> String {
  if browser == "camoufox" {
    return format!("ws://127.0.0.1:{port}/session");
  }
  let url = format!("http://127.0.0.1:{port}/json/version");
  let client = reqwest::Client::builder()
    .no_proxy()
    .timeout(std::time::Duration::from_secs(2))
    .build();
  if let Ok(client) = client {
    // The browser needs a moment after spawn before DevTools answers.
    for _ in 0..30 {
      if let Ok(resp) = client.get(&url).send().await {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
          if let Some(ws) = body.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
            return ws.to_string();
          }
        }
      }
      tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
  }
  format!("ws://127.0.0.1:{port}/devtools/browser")
}

async fn start_profile(State(state): State<ApiState>, Json(req): Json<StartRequest>) -> ApiResult {
  let profile = find_profile(&req.uuid)?;
  if crate::browser_runner::browser_is_running_for(&profile.id.to_string()) {
    return Err(ApiError(
      StatusCode::CONFLICT,
      "profile is already running".to_string(),
    ));
  }
  if let Some(candidate) = req.url.as_deref() {
    if !crate::mcp_server::is_navigable_url(candidate) {
      return Err(bad_request("url must be http, https or about:blank"));
    }
  }
  if !req.flags.is_empty() {
    log::info!(
      "Local API: ignoring {} custom flag(s) for profile {}",
      req.flags.len(),
      profile.name
    );
  }

  let port = match req.debug_port {
    DebugPort::Port(port) if port >= 1024 => port,
    DebugPort::Port(_) => return Err(bad_request("debug_port must be 1024-65534")),
    DebugPort::Flag(_) | DebugPort::Unset => free_port().await?,
  };

  crate::team_lock::acquire_team_lock_if_needed(&profile)
    .await
    .map_err(|e| ApiError(StatusCode::CONFLICT, e.to_string()))?;

  let launched = crate::browser_runner::launch_browser_profile_impl(
    state.app_handle.clone(),
    profile.clone(),
    req.url,
    crate::browser_runner::LaunchOptions::automation(Some(port), req.headless),
  )
  .await
  .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

  let session = Session {
    headless: req.headless,
    debug_port: port,
    ws_endpoint: ws_endpoint(&launched.browser, port).await,
    start_time: now_secs(),
  };
  let uuid = octo_uuid(&launched);
  if let Ok(mut sessions) = SESSIONS.lock() {
    sessions.insert(uuid.clone(), session.clone());
  }

  Ok(
    Json(ProfileState {
      uuid,
      state: "STARTED",
      headless: session.headless,
      start_time: session.start_time,
      ws_endpoint: session.ws_endpoint,
      debug_port: session.debug_port,
      browser_pid: launched.process_id,
      browser: launched.browser,
    })
    .into_response(),
  )
}

async fn stop_profile(State(state): State<ApiState>, Json(req): Json<UuidRequest>) -> ApiResult {
  let profile = find_profile(&req.uuid)?;
  crate::browser_runner::BrowserRunner::instance()
    .kill_browser_process(state.app_handle.clone(), &profile)
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  crate::team_lock::release_team_lock_if_needed(&profile).await;
  if let Ok(mut sessions) = SESSIONS.lock() {
    sessions.remove(&octo_uuid(&profile));
  }
  Ok(Json(json!({ "msg": "Profile stopped" })).into_response())
}

fn envelope(data: serde_json::Value) -> Response {
  Json(json!({ "success": true, "msg": "", "data": data, "code": null })).into_response()
}

fn profile_json(profile: &BrowserProfile) -> serde_json::Value {
  json!({
    "uuid": octo_uuid(profile),
    "title": profile.name,
    "tags": profile.tags,
    "description": profile.note,
    "browser": profile.browser,
    "version": profile.version,
    "proxy": profile.proxy_id.as_ref().map(|id| json!({ "uuid": id })),
    "status": if crate::browser_runner::browser_is_running_for(&profile.id.to_string()) { 1 } else { 0 },
    "created_at": profile.created_at,
    "updated_at": profile.updated_at,
  })
}

/// Octo's list filters. Without `page_len` the whole list is returned.
#[derive(Deserialize, Default)]
struct ListQuery {
  /// Part of a title, in any letter case.
  #[serde(default)]
  search: Option<String>,
  /// Comma-separated tags; a profile is listed when it carries all of them.
  #[serde(default)]
  search_tags: Option<String>,
  #[serde(default)]
  page: usize,
  #[serde(default)]
  page_len: Option<usize>,
}

fn matches_query(profile: &BrowserProfile, query: &ListQuery) -> bool {
  let title_matches = query.search.as_deref().is_none_or(|search| {
    profile
      .name
      .to_lowercase()
      .contains(&search.trim().to_lowercase())
  });
  let tags_match = query.search_tags.as_deref().is_none_or(|tags| {
    tags
      .split(',')
      .map(str::trim)
      .filter(|tag| !tag.is_empty())
      .all(|tag| profile.tags.iter().any(|own| own == tag))
  });
  title_matches && tags_match
}

/// The `page` of `items` that `page_len` asks for, or all of them.
fn page_of<T>(items: Vec<T>, page: usize, page_len: Option<usize>) -> Vec<T> {
  match page_len {
    Some(len) => items
      .into_iter()
      .skip(page.saturating_mul(len))
      .take(len)
      .collect(),
    None => items,
  }
}

async fn list_profiles(Query(query): Query<ListQuery>) -> ApiResult {
  let profiles: Vec<BrowserProfile> = ProfileManager::instance()
    .list_profiles()
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .filter(|p| !p.temporary && matches_query(p, &query))
    .collect();
  let total_count = profiles.len();
  let data: Vec<serde_json::Value> = page_of(profiles, query.page, query.page_len)
    .iter()
    .map(profile_json)
    .collect();
  Ok(
    Json(json!({
      "success": true,
      "msg": "",
      "data": data,
      "code": null,
      "total_count": total_count,
      "page": query.page,
    }))
    .into_response(),
  )
}

async fn get_profile(Path(uuid): Path<String>) -> ApiResult {
  Ok(envelope(profile_json(&find_profile(&uuid)?)))
}

/// A field that tells "left out" from "set to null".
fn present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
  T: Deserialize<'de>,
  D: serde::Deserializer<'de>,
{
  T::deserialize(deserializer).map(Some)
}

/// Octo's profile edit: only the fields that are sent change.
#[derive(Deserialize)]
struct UpdateProfileRequest {
  #[serde(default)]
  title: Option<String>,
  #[serde(default)]
  description: Option<String>,
  #[serde(default)]
  tags: Option<Vec<String>>,
  /// `{ "uuid": "<proxy id>" }` attaches a stored proxy, `null` detaches it.
  #[serde(default, deserialize_with = "present")]
  proxy: Option<Option<UuidRequest>>,
}

async fn update_profile(
  State(state): State<ApiState>,
  Path(uuid): Path<String>,
  Json(req): Json<UpdateProfileRequest>,
) -> ApiResult {
  let id = find_profile(&uuid)?.id.to_string();
  let manager = ProfileManager::instance();
  let app_handle = &state.app_handle;
  if let Some(title) = req.title {
    manager
      .rename_profile(app_handle, &id, &title)
      .map_err(|e| bad_request(e.to_string()))?;
  }
  if let Some(description) = req.description {
    let note = Some(description).filter(|text| !text.trim().is_empty());
    manager
      .update_profile_note(app_handle, &id, note)
      .map_err(|e| bad_request(e.to_string()))?;
  }
  if let Some(tags) = req.tags {
    manager
      .update_profile_tags(app_handle, &id, tags)
      .map_err(|e| bad_request(e.to_string()))?;
  }
  if let Some(proxy) = req.proxy {
    manager
      .update_profile_proxy(app_handle.clone(), &id, proxy.map(|p| p.uuid))
      .await
      .map_err(|e| bad_request(e.to_string()))?;
  }
  Ok(envelope(profile_json(&find_profile(&uuid)?)))
}

/// Octo's cookie import: `cookies` is a list in the browser-extension export
/// shape (`domain`, `name`, `value`, `path`, `expirationDate`, ...).
#[derive(Deserialize)]
struct ImportCookiesRequest {
  cookies: Vec<serde_json::Value>,
}

async fn import_cookies(
  State(state): State<ApiState>,
  Path(uuid): Path<String>,
  Json(req): Json<ImportCookiesRequest>,
) -> ApiResult {
  let profile = find_profile(&uuid)?;
  if crate::browser_runner::browser_is_running_for(&profile.id.to_string()) {
    return Err(ApiError(
      StatusCode::CONFLICT,
      "the profile is running; stop it before importing cookies".to_string(),
    ));
  }
  let content = serde_json::Value::Array(req.cookies).to_string();
  let result = crate::cookie_manager::CookieManager::import_cookies(
    &state.app_handle,
    &profile.id.to_string(),
    &content,
  )
  .await
  .map_err(bad_request)?;
  Ok(envelope(json!({
    "imported": result.cookies_imported,
    "replaced": result.cookies_replaced,
    "errors": result.errors,
  })))
}

/// Antidetect extension: the profile's cookies in the shape `import_cookies`
/// takes, so they can be moved between profiles by a script.
async fn export_cookies(Path(uuid): Path<String>) -> ApiResult {
  let profile = find_profile(&uuid)?;
  let exported =
    crate::cookie_manager::CookieManager::export_cookies(&profile.id.to_string(), "json")
      .map_err(bad_request)?;
  let cookies: serde_json::Value = serde_json::from_str(&exported)
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  Ok(envelope(cookies))
}

/// Tags are the names profiles carry; a tag's `uuid` is its name.
async fn list_tags() -> ApiResult {
  let tags = crate::tag_manager::TAG_MANAGER
    .lock()
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .get_all_tags()
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  Ok(envelope(json!(tags
    .iter()
    .map(|tag| json!({ "uuid": tag, "name": tag }))
    .collect::<Vec<_>>())))
}

#[derive(Deserialize, Default)]
struct OctoFingerprint {
  /// Octo's `win` / `mac` / `lin`.
  #[serde(default)]
  os: Option<String>,
}

#[derive(Deserialize)]
struct CreateProfileRequest {
  title: String,
  #[serde(default)]
  tags: Vec<String>,
  #[serde(default)]
  fingerprint: OctoFingerprint,
  /// Stored proxy to attach, as `{ "uuid": "<proxy id>" }`.
  #[serde(default)]
  proxy: Option<UuidRequest>,
  /// Antidetect extension: the engine, `camoufox` (default), `cloakbrowser`
  /// or `wayfern`.
  #[serde(default)]
  browser: Option<String>,
  /// Antidetect extension, CloakBrowser only: a video file to stand in for
  /// the camera.
  #[serde(default)]
  camera_video: Option<String>,
  /// Antidetect extension, Camoufox and CloakBrowser: the language of the
  /// browser's own interface, when it should differ from the profile's.
  #[serde(default)]
  interface_language: Option<String>,
}

fn fingerprint_os(os: Option<&str>) -> Result<Option<String>, ApiError> {
  Ok(match os.map(str::to_ascii_lowercase).as_deref() {
    None => None,
    Some("win" | "windows") => Some("windows".to_string()),
    Some("mac" | "macos") => Some("macos".to_string()),
    Some("lin" | "linux") => Some("linux".to_string()),
    Some(other) => return Err(bad_request(format!("unsupported fingerprint os: {other}"))),
  })
}

async fn create_profile(
  State(state): State<ApiState>,
  Json(req): Json<CreateProfileRequest>,
) -> ApiResult {
  let profile = create_from_request(&state.app_handle, req).await?;
  Ok(envelope(profile_json(&profile)))
}

async fn create_from_request(
  app_handle: &tauri::AppHandle,
  req: CreateProfileRequest,
) -> Result<BrowserProfile, ApiError> {
  let browser = req.browser.as_deref().unwrap_or("camoufox").to_string();
  let os = fingerprint_os(req.fingerprint.os.as_deref())?;
  let os_for_cloakbrowser = os.clone();
  let interface_language = match req.interface_language.as_deref() {
    None => None,
    Some(language) => Some(
      crate::interface_language::clean_tag(language)
        .ok_or_else(|| bad_request(format!("not a language tag: {language}")))?,
    ),
  };
  let (camoufox_config, wayfern_config) = match browser.as_str() {
    "camoufox" => (
      Some(crate::camoufox_manager::CamoufoxConfig {
        os,
        interface_language: interface_language.clone(),
        ..Default::default()
      }),
      None,
    ),
    "wayfern" => (
      None,
      Some(crate::wayfern_manager::WayfernConfig {
        os,
        ..Default::default()
      }),
    ),
    // A CloakBrowser profile is created with its own seed; the requested OS
    // is applied to that config below.
    "cloakbrowser" => (None, None),
    other => return Err(bad_request(format!("unsupported browser: {other}"))),
  };
  let cloakbrowser_os = (browser == "cloakbrowser")
    .then(|| os_for_cloakbrowser.clone())
    .flatten();
  let camera_video = req.camera_video.filter(|_| browser == "cloakbrowser");

  let mut versions = crate::downloaded_browsers_registry::DownloadedBrowsersRegistry::instance()
    .get_downloaded_versions(&browser);
  if browser == "cloakbrowser" && crate::api_client::ApiClient::cloakbrowser_license_key().is_none()
  {
    // Without a licence key a Pro build on disk cannot start.
    versions.retain(|version| !crate::api_client::ApiClient::cloakbrowser_is_pro(version));
  }
  versions.sort_by(|a, b| crate::api_client::compare_versions(b, a));
  let version = versions
    .into_iter()
    .next()
    .ok_or_else(|| bad_request(format!("{browser} is not downloaded yet")))?;

  let manager = ProfileManager::instance();
  let profile = manager
    .create_profile_with_group(
      app_handle,
      &req.title,
      &browser,
      &version,
      "stable",
      req.proxy.map(|p| p.uuid),
      None,
      camoufox_config,
      wayfern_config,
      None,
      false,
      None,
      None,
    )
    .await
    .map_err(|e| bad_request(e.to_string()))?;
  let interface_language = interface_language.filter(|_| browser == "cloakbrowser");
  let profile = match (cloakbrowser_os, camera_video, interface_language) {
    (None, None, None) => profile,
    (os, camera_video, interface_language) => {
      let current = profile.cloakbrowser_config.clone().unwrap_or_default();
      let config = crate::cloakbrowser_manager::CloakBrowserConfig {
        os: os.or(current.os.clone()),
        camera_video,
        interface_language,
        ..current
      };
      manager
        .update_cloakbrowser_config(app_handle.clone(), &profile.id.to_string(), config)
        .await
        .map_err(bad_request)?
    }
  };
  let profile = if req.tags.is_empty() {
    profile
  } else {
    manager
      .update_profile_tags(app_handle, &profile.id.to_string(), req.tags)
      .map_err(|e| bad_request(e.to_string()))?
  };
  Ok(profile)
}

/// A `start` request and a `create` request in one body: Octo's one-time
/// profile takes the profile's settings beside the launch options.
#[derive(Deserialize)]
struct OneTimeStartRequest {
  #[serde(default)]
  title: Option<String>,
  #[serde(default)]
  tags: Vec<String>,
  #[serde(default)]
  fingerprint: OctoFingerprint,
  #[serde(default)]
  proxy: Option<UuidRequest>,
  #[serde(default)]
  browser: Option<String>,
  #[serde(default)]
  headless: bool,
  #[serde(default)]
  debug_port: DebugPort,
  #[serde(default)]
  flags: Vec<String>,
  #[serde(default)]
  url: Option<String>,
}

/// Create a profile that lives for one session and start it. Its data stays
/// in memory, and the profile is deleted when its browser stops.
async fn start_one_time_profile(
  State(state): State<ApiState>,
  Json(req): Json<OneTimeStartRequest>,
) -> ApiResult {
  let title = req
    .title
    .unwrap_or_else(|| format!("One-time {}", now_secs()));
  let created = create_from_request(
    &state.app_handle,
    CreateProfileRequest {
      title,
      tags: req.tags,
      fingerprint: req.fingerprint,
      proxy: req.proxy,
      browser: req.browser,
      camera_video: None,
      interface_language: None,
    },
  )
  .await?;
  let manager = ProfileManager::instance();
  let profile = manager
    .mark_profile_temporary(&created.id.to_string())
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

  let started = start_profile(
    State(state.clone()),
    Json(StartRequest {
      uuid: octo_uuid(&profile),
      headless: req.headless,
      debug_port: req.debug_port,
      flags: req.flags,
      url: req.url,
    }),
  )
  .await;
  if started.is_err() {
    // Nothing will ever stop a browser that did not start, so nothing else
    // would delete the profile.
    if let Err(e) = manager.delete_profile_permanently(&state.app_handle, &profile.id.to_string()) {
      log::warn!("Could not delete one-time profile {}: {e}", profile.name);
    }
  }
  started
}

#[derive(Deserialize)]
struct DeleteProfilesRequest {
  uuids: Vec<String>,
}

/// Moves the profiles to the trash, like Octo's delete without `skip_trash_bin`.
async fn delete_profiles(
  State(state): State<ApiState>,
  Json(req): Json<DeleteProfilesRequest>,
) -> ApiResult {
  let profiles: Vec<BrowserProfile> = req
    .uuids
    .iter()
    .map(|uuid| find_profile(uuid))
    .collect::<Result<_, _>>()?;
  for profile in &profiles {
    ProfileManager::instance()
      .delete_profile(&state.app_handle, &profile.id.to_string())
      .map_err(|e| bad_request(e.to_string()))?;
  }
  Ok(envelope(json!({ "deleted": profiles.len() })))
}

fn proxy_json(proxy: &crate::proxy_manager::StoredProxy) -> serde_json::Value {
  json!({
    "uuid": proxy.id,
    "title": proxy.name,
    "type": proxy.proxy_settings.proxy_type,
    "host": proxy.proxy_settings.host,
    "port": proxy.proxy_settings.port,
    "login": proxy.proxy_settings.username,
  })
}

async fn list_proxies() -> ApiResult {
  let proxies = crate::proxy_manager::PROXY_MANAGER.get_stored_proxies();
  Ok(envelope(json!(proxies
    .iter()
    .map(proxy_json)
    .collect::<Vec<_>>())))
}

/// Octo's proxy shape: `type` is `http`, `https`, `socks5` (or `socks4`).
#[derive(Deserialize)]
struct CreateProxyRequest {
  title: String,
  #[serde(rename = "type")]
  proxy_type: String,
  host: String,
  port: u16,
  #[serde(default)]
  login: Option<String>,
  #[serde(default)]
  password: Option<String>,
}

async fn create_proxy(
  State(state): State<ApiState>,
  Json(req): Json<CreateProxyRequest>,
) -> ApiResult {
  let proxy_type = supported_proxy_type(&req.proxy_type)?;
  if req.host.trim().is_empty() || req.port == 0 {
    return Err(bad_request("host and port are required"));
  }
  let proxy = crate::proxy_manager::PROXY_MANAGER
    .create_stored_proxy(
      &state.app_handle,
      req.title,
      crate::browser::ProxySettings {
        proxy_type,
        host: req.host.trim().to_string(),
        port: req.port,
        username: req.login.filter(|v| !v.is_empty()),
        password: req.password.filter(|v| !v.is_empty()),
        vless_uri: None,
      },
    )
    .map_err(bad_request)?;
  Ok(envelope(proxy_json(&proxy)))
}

/// Octo's proxy edit: only the fields that are sent change.
#[derive(Deserialize)]
struct UpdateProxyRequest {
  #[serde(default)]
  title: Option<String>,
  #[serde(default, rename = "type")]
  proxy_type: Option<String>,
  #[serde(default)]
  host: Option<String>,
  #[serde(default)]
  port: Option<u16>,
  #[serde(default)]
  login: Option<String>,
  #[serde(default)]
  password: Option<String>,
}

fn supported_proxy_type(proxy_type: &str) -> Result<String, ApiError> {
  let proxy_type = proxy_type.to_ascii_lowercase();
  if matches!(
    proxy_type.as_str(),
    "http" | "https" | "socks4" | "socks5" | "ssh"
  ) {
    Ok(proxy_type)
  } else {
    Err(bad_request(format!("unsupported proxy type: {proxy_type}")))
  }
}

async fn update_proxy(
  State(state): State<ApiState>,
  Path(uuid): Path<String>,
  Json(req): Json<UpdateProxyRequest>,
) -> ApiResult {
  let current = crate::proxy_manager::PROXY_MANAGER
    .get_stored_proxies()
    .into_iter()
    .find(|proxy| proxy.id == uuid)
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("proxy {uuid} not found")))?;
  let touches_settings = req.proxy_type.is_some()
    || req.host.is_some()
    || req.port.is_some()
    || req.login.is_some()
    || req.password.is_some();
  let settings = if touches_settings {
    let mut settings = current.proxy_settings.clone();
    if let Some(proxy_type) = req.proxy_type.as_deref() {
      settings.proxy_type = supported_proxy_type(proxy_type)?;
    }
    if let Some(host) = req.host {
      settings.host = host.trim().to_string();
    }
    if let Some(port) = req.port {
      settings.port = port;
    }
    if let Some(login) = req.login {
      settings.username = Some(login).filter(|v| !v.is_empty());
    }
    if let Some(password) = req.password {
      settings.password = Some(password).filter(|v| !v.is_empty());
    }
    if settings.host.is_empty() || settings.port == 0 {
      return Err(bad_request("host and port are required"));
    }
    Some(settings)
  } else {
    None
  };
  let proxy = crate::proxy_manager::PROXY_MANAGER
    .update_stored_proxy(&state.app_handle, &uuid, req.title, settings)
    .map_err(bad_request)?;
  Ok(envelope(proxy_json(&proxy)))
}

async fn delete_proxy(State(state): State<ApiState>, Path(uuid): Path<String>) -> ApiResult {
  crate::proxy_manager::PROXY_MANAGER
    .delete_stored_proxy(&state.app_handle, &uuid)
    .map_err(bad_request)?;
  Ok(envelope(json!({ "deleted": 1 })))
}

async fn delete_proxies(
  State(state): State<ApiState>,
  Json(req): Json<DeleteProfilesRequest>,
) -> ApiResult {
  for uuid in &req.uuids {
    crate::proxy_manager::PROXY_MANAGER
      .delete_stored_proxy(&state.app_handle, uuid)
      .map_err(bad_request)?;
  }
  Ok(envelope(json!({ "deleted": req.uuids.len() })))
}

/// The robot's own request, with the profiles named the way this API names
/// them.
#[derive(Deserialize)]
struct CookieRobotBody {
  uuids: Vec<String>,
  #[serde(flatten)]
  request: crate::cookie_robot::CookieRobotRequest,
}

fn cookie_robot_json() -> serde_json::Value {
  serde_json::to_value(crate::cookie_robot::status()).unwrap_or_default()
}

/// Not part of Octo's API: start the Cookie Robot on a set of profiles.
async fn start_cookie_robot(
  State(state): State<ApiState>,
  Json(req): Json<CookieRobotBody>,
) -> ApiResult {
  let mut request = req.request;
  request.profile_ids.clear();
  for uuid in &req.uuids {
    request.profile_ids.push(find_profile(uuid)?.id.to_string());
  }
  crate::cookie_robot::start(state.app_handle.clone(), request).map_err(|e| {
    let status = if e.contains("COOKIE_ROBOT_ALREADY_RUNNING") {
      StatusCode::CONFLICT
    } else {
      StatusCode::BAD_REQUEST
    };
    ApiError(status, e)
  })?;
  Ok(envelope(cookie_robot_json()))
}

#[derive(Deserialize)]
struct SynchronizerStartRequest {
  /// The profile whose input is mirrored.
  leader: String,
  followers: Vec<String>,
  #[serde(default)]
  headless: bool,
}

#[derive(Deserialize)]
struct SynchronizerStopRequest {
  session_id: String,
}

/// Not part of Octo's API: start the profiles and mirror the leader's input
/// in the followers.
async fn start_synchronizer(
  State(state): State<ApiState>,
  Json(req): Json<SynchronizerStartRequest>,
) -> ApiResult {
  let leader = find_profile(&req.leader)?.id.to_string();
  let mut followers = Vec::new();
  for uuid in &req.followers {
    followers.push(find_profile(uuid)?.id.to_string());
  }
  let session = crate::synchronizer::SynchronizerManager::instance()
    .start_session_with(state.app_handle.clone(), leader, followers, req.headless)
    .await
    .map_err(bad_request)?;
  Ok(envelope(json!(session)))
}

async fn stop_synchronizer(
  State(state): State<ApiState>,
  Json(req): Json<SynchronizerStopRequest>,
) -> ApiResult {
  crate::synchronizer::SynchronizerManager::instance()
    .stop_session(state.app_handle.clone(), &req.session_id)
    .await
    .map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
  Ok(envelope(json!({ "stopped": true })))
}

async fn cookie_robot_status() -> ApiResult {
  Ok(envelope(cookie_robot_json()))
}

async fn stop_cookie_robot() -> ApiResult {
  let stopped = crate::cookie_robot::stop();
  Ok(envelope(json!({ "stopped": stopped })))
}

/// The debug port of a running profile this API did not start itself (the
/// app restarted, or the profile was launched from the UI), read back from the
/// browser process.
async fn running_debug_port(profile: &BrowserProfile) -> Option<u16> {
  let profiles_dir = ProfileManager::instance().get_profiles_dir();
  let path = crate::ephemeral_dirs::get_effective_profile_path(profile, &profiles_dir);
  let path = path.to_string_lossy();
  match profile.browser.as_str() {
    "camoufox" => crate::camoufox_manager::CamoufoxManager::instance()
      .find_camoufox_by_profile(&path)
      .await
      .ok()
      .flatten()
      .and_then(|instance| instance.cdp_port),
    "cloakbrowser" => {
      crate::cloakbrowser_manager::find_by_profile(&path).and_then(|process| process.cdp_port)
    }
    _ => None,
  }
}

async fn active_profiles() -> ApiResult {
  let profiles = ProfileManager::instance()
    .list_profiles()
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
  let sessions = SESSIONS.lock().map(|s| s.clone()).unwrap_or_default();
  let mut active = Vec::new();
  for profile in profiles {
    if !crate::browser_runner::browser_is_running_for(&profile.id.to_string()) {
      continue;
    }
    let uuid = octo_uuid(&profile);
    let session = match sessions.get(&uuid) {
      Some(session) => session.clone(),
      None => {
        let debug_port = running_debug_port(&profile).await.unwrap_or(0);
        Session {
          headless: false,
          debug_port,
          ws_endpoint: if debug_port == 0 {
            String::new()
          } else {
            ws_endpoint(&profile.browser, debug_port).await
          },
          start_time: profile.last_launch.unwrap_or(0),
        }
      }
    };
    active.push(ProfileState {
      state: "STARTED",
      headless: session.headless,
      start_time: session.start_time,
      ws_endpoint: session.ws_endpoint,
      debug_port: session.debug_port,
      browser_pid: profile.process_id,
      browser: profile.browser.clone(),
      uuid,
    });
  }
  Ok(Json(active).into_response())
}

/// Reject requests addressed to anything but a loopback host name. A page on
/// a rebound DNS name reaches this socket with its own name in `Host`.
async fn require_loopback_host(request: Request, next: Next) -> Response {
  let host = request
    .headers()
    .get(header::HOST)
    .and_then(|h| h.to_str().ok())
    .unwrap_or_default();
  let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
  if matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
    next.run(request).await
  } else {
    ApiError(StatusCode::FORBIDDEN, "forbidden host".to_string()).into_response()
  }
}

fn router(app_handle: tauri::AppHandle) -> Router {
  Router::new()
    .route("/api/profiles/start", post(start_profile))
    .route("/api/profiles/stop", post(stop_profile))
    .route("/api/profiles/force_stop", post(stop_profile))
    .route("/api/profiles/active", get(active_profiles))
    .route("/api/profiles/one_time/start", post(start_one_time_profile))
    .route(
      "/api/v2/automation/profiles",
      get(list_profiles)
        .post(create_profile)
        .delete(delete_profiles),
    )
    .route(
      "/api/v2/automation/profiles/{uuid}",
      get(get_profile).patch(update_profile),
    )
    .route(
      "/api/v2/automation/profiles/{uuid}/import_cookies",
      post(import_cookies),
    )
    .route(
      "/api/v2/automation/profiles/{uuid}/cookies",
      get(export_cookies),
    )
    .route("/api/v2/automation/tags", get(list_tags))
    .route(
      "/api/v2/automation/proxies",
      get(list_proxies).post(create_proxy).delete(delete_proxies),
    )
    .route(
      "/api/v2/automation/proxies/{uuid}",
      patch(update_proxy).delete(delete_proxy),
    )
    .route(
      "/api/v2/automation/synchronizer",
      post(start_synchronizer).delete(stop_synchronizer),
    )
    .route(
      "/api/v2/automation/cookie_robot",
      get(cookie_robot_status)
        .post(start_cookie_robot)
        .delete(stop_cookie_robot),
    )
    .layer(middleware::from_fn(require_loopback_host))
    .with_state(ApiState { app_handle })
}

/// Serve the API for the life of the app. A busy port (for example Octo
/// Browser itself running) is logged and skipped. `ANTIDETECT_LOCAL_API_PORT`
/// overrides the port; `0` turns the API off.
pub fn spawn(app_handle: tauri::AppHandle) {
  let port = std::env::var("ANTIDETECT_LOCAL_API_PORT")
    .ok()
    .and_then(|v| v.parse::<u16>().ok())
    .unwrap_or(DEFAULT_PORT);
  if port == 0 {
    log::info!("Octo-compatible local API disabled");
    return;
  }
  tauri::async_runtime::spawn(async move {
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
      Ok(listener) => listener,
      Err(e) => {
        log::warn!("Octo-compatible local API not started on 127.0.0.1:{port}: {e}");
        return;
      }
    };
    log::info!("Octo-compatible local API listening on 127.0.0.1:{port}");
    if let Err(e) = axum::serve(listener, router(app_handle)).await {
      log::error!("Octo-compatible local API stopped: {e}");
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ids_match_with_or_without_dashes() {
    assert_eq!(
      normalize_id("5A1B2C3D-0000-4000-8000-00000000ABCD"),
      "5a1b2c3d00004000800000000000abcd"
    );
    assert_eq!(
      normalize_id(" 5a1b2c3d00004000800000000000abcd "),
      "5a1b2c3d00004000800000000000abcd"
    );
  }

  #[test]
  fn octo_os_names_map_to_fingerprint_os() {
    assert_eq!(
      fingerprint_os(Some("win")).ok().flatten().as_deref(),
      Some("windows")
    );
    assert_eq!(
      fingerprint_os(Some("mac")).ok().flatten().as_deref(),
      Some("macos")
    );
    assert_eq!(fingerprint_os(None).ok().flatten(), None);
    assert!(fingerprint_os(Some("android")).is_err());
  }

  #[test]
  fn a_profile_edit_tells_a_detached_proxy_from_an_untouched_one() {
    let untouched: UpdateProfileRequest = serde_json::from_str(r#"{"title":"New"}"#).unwrap();
    assert!(untouched.proxy.is_none());
    assert_eq!(untouched.title.as_deref(), Some("New"));
    let detached: UpdateProfileRequest = serde_json::from_str(r#"{"proxy":null}"#).unwrap();
    assert!(matches!(detached.proxy, Some(None)));
    let attached: UpdateProfileRequest =
      serde_json::from_str(r#"{"proxy":{"uuid":"p1"}}"#).unwrap();
    assert_eq!(
      attached.proxy.flatten().map(|p| p.uuid).as_deref(),
      Some("p1")
    );
  }

  #[test]
  fn a_page_is_cut_only_when_a_length_is_asked_for() {
    let items: Vec<u32> = (0..7).collect();
    assert_eq!(page_of(items.clone(), 0, None), items);
    assert_eq!(page_of(items.clone(), 1, Some(3)), vec![3, 4, 5]);
    assert_eq!(page_of(items.clone(), 2, Some(3)), vec![6]);
    assert!(page_of(items, 9, Some(3)).is_empty());
  }

  #[test]
  fn proxy_types_are_checked_in_any_letter_case() {
    assert_eq!(
      supported_proxy_type("SOCKS5").ok().as_deref(),
      Some("socks5")
    );
    assert_eq!(supported_proxy_type("ssh").ok().as_deref(), Some("ssh"));
    assert!(supported_proxy_type("vless").is_err());
  }

  #[test]
  fn debug_port_accepts_octo_shapes() {
    let flag: StartRequest = serde_json::from_str(r#"{"uuid":"x","debug_port":true}"#).unwrap();
    assert!(matches!(flag.debug_port, DebugPort::Flag(true)));
    let port: StartRequest = serde_json::from_str(r#"{"uuid":"x","debug_port":9222}"#).unwrap();
    assert!(matches!(port.debug_port, DebugPort::Port(9222)));
    let unset: StartRequest = serde_json::from_str(r#"{"uuid":"x"}"#).unwrap();
    assert!(matches!(unset.debug_port, DebugPort::Unset));
    assert!(!unset.headless);
  }
}
