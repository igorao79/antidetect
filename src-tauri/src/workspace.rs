//! Your own Owl server (fork). Anyone can run one (`donut-sync/`, in Docker)
//! and invite people to it; its members share the profiles synced there and
//! take turns in them. The app joins servers with an invite link
//! `owl://join?server=<url>&code=<code>` and syncs with one of them at a time
//! (`workspace_servers.rs` keeps the others and switches between them).
//!
//! Joining stores the server as the self-hosted sync server and the personal
//! token the server hands out as the sync token, so the existing sync engine
//! does the syncing. What this module adds on top: the member's identity and
//! role (`workspace.json`), the members and invites calls, the profile locks
//! the Donut cloud used to answer, and who may take a profile off the server.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::cloud_auth::{AuthorizedError, CLOUD_API_URL, CLOUD_AUTH};
use crate::profile::BrowserProfile;
use crate::settings_manager::SettingsManager;

const STATE_FILE: &str = "workspace.json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceMember {
  pub id: String,
  pub name: String,
  pub role: String,
}

impl WorkspaceMember {
  pub fn is_owner(&self) -> bool {
    self.role == "owner"
  }
}

/// The server this app has joined, as it introduced itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceInfo {
  pub server: String,
  pub name: String,
  pub member: WorkspaceMember,
}

/// Who shared a profile to the server. Travels with the profile's metadata,
/// so every member sees it and the app can tell whose profile it is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharedBy {
  pub id: String,
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInvite {
  pub id: String,
  #[serde(default)]
  pub code: Option<String>,
  #[serde(default)]
  pub link: Option<String>,
  #[serde(default)]
  pub created_at: Option<String>,
  pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberEntry {
  pub id: String,
  pub name: String,
  pub role: String,
  #[serde(default)]
  pub created_at: Option<String>,
}

#[derive(Deserialize)]
struct WorkspaceName {
  name: String,
}

#[derive(Deserialize)]
struct JoinResponse {
  token: String,
  member: WorkspaceMember,
  workspace: WorkspaceName,
}

#[derive(Deserialize)]
struct MeResponse {
  member: WorkspaceMember,
  workspace: WorkspaceName,
}

#[derive(Deserialize)]
struct MembersResponse {
  members: Vec<WorkspaceMemberEntry>,
}

#[derive(Deserialize)]
struct InvitesResponse {
  invites: Vec<WorkspaceInvite>,
}

fn error(code: &str) -> String {
  crate::backend_error(code)
}

fn state_path() -> PathBuf {
  crate::app_dirs::data_dir().join(STATE_FILE)
}

fn read_state() -> Option<WorkspaceInfo> {
  let text = std::fs::read_to_string(state_path()).ok()?;
  serde_json::from_str(&text).ok()
}

fn write_state(info: &WorkspaceInfo) -> Result<(), String> {
  let path = state_path();
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  let text = serde_json::to_string_pretty(info).map_err(|e| e.to_string())?;
  let tmp = path.with_extension("json.tmp");
  std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
  std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// The server this app has joined, if any.
pub fn current() -> Option<WorkspaceInfo> {
  read_state()
}

/// The server address and invite code in an invite link. Takes the link as
/// the server prints and the app copies it, also with spaces around it.
pub fn parse_invite(link: &str) -> Option<(String, String)> {
  let url = url::Url::parse(link.trim()).ok()?;
  if url.scheme() != "owl" || url.host_str() != Some("join") {
    return None;
  }
  let mut server = None;
  let mut code = None;
  for (key, value) in url.query_pairs() {
    match key.as_ref() {
      "server" => server = Some(value.to_string()),
      "code" => code = Some(value.to_string()),
      _ => {}
    }
  }
  let server = clean_server(&server?)?;
  let code = code
    .map(|c| c.trim().to_string())
    .filter(|c| !c.is_empty())?;
  Some((server, code))
}

/// A server address as the app stores it: http(s), no trailing slash.
fn clean_server(raw: &str) -> Option<String> {
  let url = url::Url::parse(raw.trim()).ok()?;
  if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
    return None;
  }
  Some(url.as_str().trim_end_matches('/').to_string())
}

fn clean_name(raw: &str) -> Option<String> {
  let name: String = raw.trim().chars().take(MAX_NAME_CHARS).collect();
  (!name.is_empty()).then_some(name)
}

fn http() -> reqwest::Client {
  reqwest::Client::builder()
    .timeout(REQUEST_TIMEOUT)
    .build()
    .unwrap_or_default()
}

async fn member_token(app_handle: &tauri::AppHandle) -> Option<String> {
  SettingsManager::instance()
    .get_sync_token(app_handle)
    .await
    .ok()
    .flatten()
}

/// Send one call to the joined server with this member's token.
pub(crate) async fn call(
  app_handle: &tauri::AppHandle,
  method: reqwest::Method,
  path: &str,
  body: Option<serde_json::Value>,
) -> Result<reqwest::Response, String> {
  let info = current().ok_or_else(|| error("WORKSPACE_NOT_JOINED"))?;
  let token = member_token(app_handle)
    .await
    .ok_or_else(|| error("WORKSPACE_NOT_JOINED"))?;
  let mut request = http()
    .request(method, format!("{}{path}", info.server))
    .bearer_auth(token);
  if let Some(body) = body {
    request = request.json(&body);
  }
  let response = request.send().await.map_err(|e| {
    log::warn!("The Owl server did not answer {path}: {e}");
    error("WORKSPACE_UNREACHABLE")
  })?;
  match response.status().as_u16() {
    401 => Err(error("WORKSPACE_ACCESS_REVOKED")),
    403 => Err(error("WORKSPACE_OWNER_ONLY")),
    status if !(200..300).contains(&status) => {
      log::warn!("The Owl server answered {path} with {status}");
      Err(error("WORKSPACE_REQUEST_FAILED"))
    }
    _ => Ok(response),
  }
}

pub(crate) async fn json<T: serde::de::DeserializeOwned>(
  response: reqwest::Response,
) -> Result<T, String> {
  response.json().await.map_err(|e| {
    log::warn!("Unexpected answer from the Owl server: {e}");
    error("WORKSPACE_REQUEST_FAILED")
  })
}

/// Start syncing with the joined server and watch its locks.
async fn start(app_handle: tauri::AppHandle) {
  crate::sync::start_pipeline(app_handle).await;
  crate::team_lock::PROFILE_LOCK.disconnect().await;
  crate::team_lock::PROFILE_LOCK.connect().await;
}

static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Called once at startup. Watches the joined server's locks from launch, so
/// a profile a colleague has open shows as taken before anyone tries to start
/// it.
pub fn init(app_handle: tauri::AppHandle) {
  let _ = APP_HANDLE.set(app_handle);
  if current().is_none() {
    return;
  }
  tauri::async_runtime::spawn(async {
    if !CLOUD_AUTH.is_logged_in().await {
      crate::team_lock::PROFILE_LOCK.connect().await;
    }
  });
}

/// Whether this app syncs with a joined Owl server rather than Donut's cloud.
async fn joined() -> Option<WorkspaceInfo> {
  if CLOUD_AUTH.is_logged_in().await {
    return None;
  }
  current()
}

// --- Profile locks ---------------------------------------------------------

/// One profile-lock call (`path` is "", "/<id>" or "/<id>/heartbeat"), to the
/// joined Owl server, which speaks the same protocol as the Donut cloud, or
/// else to the cloud.
pub(crate) async fn lock_request(
  method: reqwest::Method,
  path: &str,
) -> Result<reqwest::Response, AuthorizedError> {
  if let Some(info) = joined().await {
    let app_handle = APP_HANDLE.get().ok_or(AuthorizedError::SignedOut)?;
    let token = member_token(app_handle)
      .await
      .ok_or(AuthorizedError::SignedOut)?;
    let response = http()
      .request(method, format!("{}/v1/profile-locks{path}", info.server))
      .bearer_auth(token)
      .send()
      .await
      .map_err(|e| AuthorizedError::Transport(e.to_string()))?;
    return match response.status() {
      reqwest::StatusCode::UNAUTHORIZED => Err(AuthorizedError::SignedOut),
      reqwest::StatusCode::TOO_MANY_REQUESTS => Err(AuthorizedError::RateLimited(
        crate::cloud_auth::retry_after(response.headers()),
      )),
      _ => Ok(response),
    };
  }
  let url = format!("{CLOUD_API_URL}/api/profile-locks{path}");
  CLOUD_AUTH
    .authorized_request(|client, token| client.request(method.clone(), &url).bearer_auth(token))
    .await
}

/// Whether profiles are locked while they run: on a joined Owl server always,
/// on the Donut cloud with a paid plan.
pub(crate) async fn locks_apply() -> bool {
  joined().await.is_some() || CLOUD_AUTH.has_active_paid_subscription().await
}

/// The id and the readable name a lock taken by this app carries.
pub(crate) async fn lock_holder() -> Option<(String, String)> {
  if let Some(info) = joined().await {
    return Some((info.member.id, info.member.name));
  }
  CLOUD_AUTH
    .get_user()
    .await
    .map(|user| (user.user.id.clone(), user.user.email.clone()))
}

// --- Who may take a profile off the server --------------------------------

/// Mark a profile as shared by this member the first time it goes to the
/// server. Returns whether the profile changed.
pub fn stamp_shared_by(profile: &mut BrowserProfile) -> bool {
  if profile.shared_by.is_some() {
    return false;
  }
  let Some(info) = current() else {
    return false;
  };
  profile.shared_by = Some(SharedBy {
    id: info.member.id,
    name: info.member.name,
  });
  profile.updated_at = Some(crate::proxy_manager::now_secs());
  true
}

/// Refuse taking a shared profile off the server (turning its sync off, or
/// deleting it) for a member who neither shared it nor owns the server. The
/// server refuses that delete too; refusing here keeps the profile as it is
/// instead of leaving a delete the server will never accept.
pub fn may_unshare(profile: &BrowserProfile) -> Result<(), String> {
  let Some(info) = current() else {
    return Ok(());
  };
  if !profile.is_sync_enabled() || info.member.is_owner() {
    return Ok(());
  }
  match &profile.shared_by {
    Some(shared_by) if shared_by.id != info.member.id => Err(
      serde_json::json!({
        "code": "WORKSPACE_PROFILE_NOT_YOURS",
        "params": { "name": shared_by.name }
      })
      .to_string(),
    ),
    _ => Ok(()),
  }
}

// --- Invite links opened from outside --------------------------------------

static PENDING_LINK: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// An `owl://join` link opened from a browser or a messenger: bring the app
/// forward and hand the link to the server page, now or when it next asks
/// (`take_pending_workspace_link`), whichever comes first.
pub fn open_join_link(app_handle: &tauri::AppHandle, link: String) {
  use tauri::Manager;
  log::info!("An Owl server invite link was opened");
  if let Ok(mut pending) = PENDING_LINK.lock() {
    *pending = Some(link.clone());
  }
  if let Some(window) = app_handle.get_webview_window("main") {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
  }
  let _ = crate::events::emit("workspace-join-link", link);
}

#[tauri::command]
pub fn take_pending_workspace_link() -> Option<String> {
  PENDING_LINK
    .lock()
    .ok()
    .and_then(|mut pending| pending.take())
}

// --- Commands --------------------------------------------------------------

#[tauri::command]
pub async fn get_workspace() -> Result<Option<WorkspaceInfo>, String> {
  Ok(current())
}

/// Join a server by its invite link, or by its address and an invite code.
#[tauri::command]
pub async fn join_workspace(
  app_handle: tauri::AppHandle,
  link: Option<String>,
  server: Option<String>,
  code: Option<String>,
  name: String,
) -> Result<WorkspaceInfo, String> {
  let (server, code) = match (link, server, code) {
    (Some(link), _, _) if !link.trim().is_empty() => {
      parse_invite(&link).ok_or_else(|| error("WORKSPACE_INVITE_LINK_INVALID"))?
    }
    (_, Some(server), Some(code)) => (
      clean_server(&server).ok_or_else(|| error("WORKSPACE_INVITE_LINK_INVALID"))?,
      code.trim().to_string(),
    ),
    _ => return Err(error("WORKSPACE_INVITE_LINK_INVALID")),
  };
  let name = clean_name(&name).ok_or_else(|| error("NAME_CANNOT_BE_EMPTY"))?;
  if CLOUD_AUTH.is_logged_in().await {
    return Err(error("SELF_HOSTED_REQUIRES_LOGOUT"));
  }
  // Checked before the invite is spent: an invite works once.
  let switching = current().is_some();
  if switching {
    crate::workspace_servers::refuse_while_shared_profiles_run()?;
  }

  let response = http()
    .post(format!("{server}/v1/join"))
    .json(&serde_json::json!({ "code": code, "name": name }))
    .send()
    .await
    .map_err(|e| {
      log::warn!("Could not reach the Owl server to join: {e}");
      error("WORKSPACE_UNREACHABLE")
    })?;
  match response.status().as_u16() {
    401 | 404 | 410 => return Err(error("WORKSPACE_INVITE_INVALID")),
    429 => return Err(error("WORKSPACE_TOO_MANY_ATTEMPTS")),
    status if !(200..300).contains(&status) => {
      log::warn!("The Owl server refused the join with {status}");
      return Err(error("WORKSPACE_REQUEST_FAILED"));
    }
    _ => {}
  }
  let joined: JoinResponse = json(response).await?;

  // The server this app syncs with now waits in the list, its profiles
  // parked, until the person switches back to it.
  if switching {
    crate::workspace_servers::deactivate(&app_handle, false).await?;
  }
  let info = WorkspaceInfo {
    server,
    name: joined.workspace.name,
    member: joined.member,
  };
  activate(&app_handle, info.clone(), &joined.token).await?;
  log::info!(
    "Joined the Owl server {} as {}",
    info.name,
    info.member.role
  );
  Ok(info)
}

/// Make `info` the server this app syncs with, under the member's `token`.
pub(crate) async fn activate(
  app_handle: &tauri::AppHandle,
  info: WorkspaceInfo,
  token: &str,
) -> Result<(), String> {
  let manager = SettingsManager::instance();
  manager
    .store_sync_token(app_handle, token)
    .await
    .map_err(|e| e.to_string())?;
  manager
    .save_sync_server_url(Some(info.server.clone()))
    .map_err(|e| e.to_string())?;
  write_state(&info)?;
  crate::workspace_servers::remember_active(app_handle).await?;
  tauri::async_runtime::spawn(start(app_handle.clone()));
  let _ = crate::events::emit("workspace-changed", ());
  Ok(())
}

/// Stop syncing with the active server and drop it as the sync server. What
/// happens to its profiles is the caller's (`workspace_servers::deactivate`).
pub(crate) async fn disconnect(app_handle: &tauri::AppHandle) -> Result<(), String> {
  crate::team_lock::PROFILE_LOCK
    .release_all_held_within(Duration::from_secs(2))
    .await;
  crate::team_lock::PROFILE_LOCK.disconnect().await;
  crate::sync::stop_pipeline();
  let manager = SettingsManager::instance();
  manager
    .save_sync_server_url(None)
    .map_err(|e| e.to_string())?;
  manager
    .remove_sync_token(app_handle)
    .await
    .map_err(|e| e.to_string())?;
  match std::fs::remove_file(state_path()) {
    Ok(()) => {}
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(e.to_string()),
  }
  let _ = crate::events::emit("workspace-changed", ());
  Ok(())
}

/// Read who this member is from the server again: a name or role changed
/// there, or the member was removed (`WORKSPACE_ACCESS_REVOKED`).
#[tauri::command]
pub async fn refresh_workspace(app_handle: tauri::AppHandle) -> Result<WorkspaceInfo, String> {
  let me: MeResponse = json(call(&app_handle, reqwest::Method::GET, "/v1/me", None).await?).await?;
  let mut info = current().ok_or_else(|| error("WORKSPACE_NOT_JOINED"))?;
  if info.member != me.member || info.name != me.workspace.name {
    info.member = me.member;
    info.name = me.workspace.name;
    write_state(&info)?;
  }
  Ok(info)
}

#[tauri::command]
pub async fn list_workspace_members(
  app_handle: tauri::AppHandle,
) -> Result<Vec<WorkspaceMemberEntry>, String> {
  let response = call(&app_handle, reqwest::Method::GET, "/v1/members", None).await?;
  Ok(json::<MembersResponse>(response).await?.members)
}

#[tauri::command]
pub async fn remove_workspace_member(
  app_handle: tauri::AppHandle,
  member_id: String,
) -> Result<(), String> {
  let path = format!("/v1/members/{}", urlencoding::encode(&member_id));
  call(&app_handle, reqwest::Method::DELETE, &path, None).await?;
  Ok(())
}

#[tauri::command]
pub async fn create_workspace_invite(
  app_handle: tauri::AppHandle,
) -> Result<WorkspaceInvite, String> {
  let body = serde_json::json!({ "role": "member" });
  json(
    call(
      &app_handle,
      reqwest::Method::POST,
      "/v1/invites",
      Some(body),
    )
    .await?,
  )
  .await
}

#[tauri::command]
pub async fn list_workspace_invites(
  app_handle: tauri::AppHandle,
) -> Result<Vec<WorkspaceInvite>, String> {
  let response = call(&app_handle, reqwest::Method::GET, "/v1/invites", None).await?;
  Ok(json::<InvitesResponse>(response).await?.invites)
}

#[tauri::command]
pub async fn revoke_workspace_invite(
  app_handle: tauri::AppHandle,
  invite_id: String,
) -> Result<(), String> {
  let path = format!("/v1/invites/{}", urlencoding::encode(&invite_id));
  call(&app_handle, reqwest::Method::DELETE, &path, None).await?;
  Ok(())
}

/// Leave the server for good: it comes off the list, and its profiles stay on
/// this computer as local copies that sync nowhere.
#[tauri::command]
pub async fn leave_workspace(app_handle: tauri::AppHandle) -> Result<(), String> {
  crate::workspace_servers::deactivate(&app_handle, true).await?;
  log::info!("Left the Owl server");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_invite_link_gives_the_server_and_the_code() {
    let link = "owl://join?server=https%3A%2F%2Fowl.example.com%2F&code=abc123";
    assert_eq!(
      parse_invite(link),
      Some(("https://owl.example.com".to_string(), "abc123".to_string()))
    );
    assert_eq!(
      parse_invite("  owl://join?code=x&server=http://192.168.1.5:12342  "),
      Some(("http://192.168.1.5:12342".to_string(), "x".to_string()))
    );
  }

  #[test]
  fn anything_else_is_not_an_invite() {
    for link in [
      "",
      "https://owl.example.com/join?server=https://a&code=x",
      "owl://join?server=https://a",
      "owl://join?code=x",
      "owl://join?server=ftp://a&code=x",
      "owl://other?server=https://a&code=x",
    ] {
      assert_eq!(parse_invite(link), None, "{link}");
    }
  }

  #[test]
  fn a_name_is_trimmed_and_kept_short() {
    assert_eq!(clean_name("  Ivan  ").as_deref(), Some("Ivan"));
    assert_eq!(clean_name("   "), None);
    assert_eq!(clean_name(&"x".repeat(100)).map(|n| n.len()), Some(64));
  }

  fn shared_profile(by: Option<&str>) -> BrowserProfile {
    BrowserProfile {
      sync_mode: crate::profile::types::SyncMode::Regular,
      shared_by: by.map(|id| SharedBy {
        id: id.to_string(),
        name: format!("Member {id}"),
      }),
      ..Default::default()
    }
  }

  #[test]
  fn only_the_sharer_or_the_owner_takes_a_profile_off_the_server() {
    let _guard = crate::app_dirs::set_test_data_dir(tempfile::tempdir().unwrap().keep());
    let mut info = WorkspaceInfo {
      server: "https://owl.example.com".to_string(),
      name: "Team".to_string(),
      member: WorkspaceMember {
        id: "me".to_string(),
        name: "Me".to_string(),
        role: "member".to_string(),
      },
    };
    write_state(&info).unwrap();
    assert!(may_unshare(&shared_profile(Some("me"))).is_ok());
    assert!(may_unshare(&shared_profile(None)).is_ok());
    let refused = may_unshare(&shared_profile(Some("other"))).unwrap_err();
    assert!(refused.contains("WORKSPACE_PROFILE_NOT_YOURS"), "{refused}");
    assert!(refused.contains("Member other"), "{refused}");

    info.member.role = "owner".to_string();
    write_state(&info).unwrap();
    assert!(may_unshare(&shared_profile(Some("other"))).is_ok());
  }

  #[test]
  fn a_profile_is_stamped_with_its_sharer_once() {
    let _guard = crate::app_dirs::set_test_data_dir(tempfile::tempdir().unwrap().keep());
    let mut profile = BrowserProfile::default();
    assert!(
      !stamp_shared_by(&mut profile),
      "nothing joined, nothing stamped"
    );
    write_state(&WorkspaceInfo {
      server: "https://owl.example.com".to_string(),
      name: "Team".to_string(),
      member: WorkspaceMember {
        id: "me".to_string(),
        name: "Me".to_string(),
        role: "member".to_string(),
      },
    })
    .unwrap();
    assert!(stamp_shared_by(&mut profile));
    assert_eq!(
      profile.shared_by.as_ref().map(|s| s.id.as_str()),
      Some("me")
    );
    assert!(profile.updated_at.is_some());
    assert!(!stamp_shared_by(&mut profile), "a stamp is never replaced");
  }
}
