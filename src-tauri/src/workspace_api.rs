//! The Owl server's API for scripts (fork): API keys the owner creates on the
//! Integrations page, and a check that a key works. A key manages the team
//! (members, invites, the list of shared profiles and who has which open) and
//! never reaches the synced data. The server keeps only a key's hash; the app
//! that created it keeps the key sealed, so the owner can copy it again.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

use crate::workspace::{call, current, json};

const KEYS_DIR: &str = "workspace-api-keys";
const KEY_MAGIC: &[u8; 6] = b"OWLAK1";
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// An API key as the server lists it; `stored` says this app holds the key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceApiKey {
  pub id: String,
  pub name: String,
  pub created_at: String,
  #[serde(default)]
  pub last_used_at: Option<String>,
  #[serde(default)]
  pub stored: bool,
}

#[derive(Deserialize)]
struct KeysResponse {
  keys: Vec<WorkspaceApiKey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedKey {
  id: String,
  name: String,
  key: String,
  created_at: String,
}

/// What a key check found, stage by stage, in the shape the Integrations
/// page's connection check reads (`check_integration_connection`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceApiCheck {
  pub configured: bool,
  pub reachable: Option<bool>,
  pub authorized: Option<bool>,
  pub http_status: Option<u16>,
  pub checked_at: u64,
}

fn error(code: &str) -> String {
  crate::backend_error(code)
}

fn key_path(server: &str, id: &str) -> PathBuf {
  let digest = Sha256::digest(server.as_bytes());
  let server_dir: String = digest[..12].iter().map(|b| format!("{b:02x}")).collect();
  let id: String = id
    .chars()
    .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
    .collect();
  crate::app_dirs::data_dir()
    .join(KEYS_DIR)
    .join(server_dir)
    .join(format!("{id}.key"))
}

fn stored_key(server: &str, id: &str) -> Option<String> {
  crate::vault::open(&key_path(server, id), KEY_MAGIC)
    .ok()
    .flatten()
}

fn server() -> Result<String, String> {
  current()
    .map(|info| info.server)
    .ok_or_else(|| error("WORKSPACE_NOT_JOINED"))
}

#[tauri::command]
pub async fn list_workspace_api_keys(
  app_handle: tauri::AppHandle,
) -> Result<Vec<WorkspaceApiKey>, String> {
  let server = server()?;
  let response = call(&app_handle, reqwest::Method::GET, "/v1/api-keys", None).await?;
  let mut keys = json::<KeysResponse>(response).await?.keys;
  for key in &mut keys {
    key.stored = key_path(&server, &key.id).exists();
  }
  Ok(keys)
}

/// Create a key on the server and keep it on this computer. Answers the key
/// itself once, for the person to copy.
#[tauri::command]
pub async fn create_workspace_api_key(
  app_handle: tauri::AppHandle,
  name: String,
) -> Result<String, String> {
  let server = server()?;
  let name: String = name.trim().chars().take(64).collect();
  if name.is_empty() {
    return Err(error("NAME_CANNOT_BE_EMPTY"));
  }
  let body = serde_json::json!({ "name": name });
  let response = call(
    &app_handle,
    reqwest::Method::POST,
    "/v1/api-keys",
    Some(body),
  )
  .await?;
  let created: CreatedKey = json(response).await?;
  crate::vault::seal(&key_path(&server, &created.id), KEY_MAGIC, &created.key)?;
  log::info!(
    "Created the Owl server API key {} ({}) at {}",
    created.name,
    created.id,
    created.created_at
  );
  Ok(created.key)
}

/// The key this app keeps, for copying it again.
#[tauri::command]
pub async fn get_workspace_api_key(id: String) -> Result<String, String> {
  stored_key(&server()?, &id).ok_or_else(|| error("WORKSPACE_API_KEY_NOT_STORED"))
}

#[tauri::command]
pub async fn revoke_workspace_api_key(
  app_handle: tauri::AppHandle,
  id: String,
) -> Result<(), String> {
  let server = server()?;
  let path = format!("/v1/api-keys/{}", urlencoding::encode(&id));
  call(&app_handle, reqwest::Method::DELETE, &path, None).await?;
  let _ = std::fs::remove_file(key_path(&server, &id));
  Ok(())
}

/// Check a key the way a script would use it: the server answers, and it
/// takes the key.
#[tauri::command]
pub async fn check_workspace_api_key(id: String) -> Result<WorkspaceApiCheck, String> {
  let server = server()?;
  let mut check = WorkspaceApiCheck {
    configured: false,
    reachable: None,
    authorized: None,
    http_status: None,
    checked_at: crate::proxy_manager::now_secs(),
  };
  let Some(key) = stored_key(&server, &id) else {
    return Ok(check);
  };
  check.configured = true;
  let client = reqwest::Client::builder()
    .timeout(CHECK_TIMEOUT)
    .build()
    .unwrap_or_default();
  let reachable = client
    .get(format!("{server}/health"))
    .send()
    .await
    .is_ok_and(|r| r.status().is_success());
  check.reachable = Some(reachable);
  if reachable {
    match client
      .get(format!("{server}/v1/members"))
      .bearer_auth(key)
      .send()
      .await
    {
      Ok(response) => {
        check.http_status = Some(response.status().as_u16());
        check.authorized = Some(response.status().is_success());
      }
      Err(_) => check.reachable = Some(false),
    }
  }
  Ok(check)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_key_file_belongs_to_its_server_and_cannot_leave_its_folder() {
    let a = key_path("https://owl.example.com", "k1");
    assert_ne!(a, key_path("https://other.example.com", "k1"));
    let odd = key_path("https://owl.example.com", "../../etc/passwd");
    assert_eq!(odd.file_name().unwrap(), "etcpasswd.key");
    assert_eq!(odd.parent(), a.parent());
  }
}
