//! The Owl servers this app has joined (fork). One is active at a time, as
//! the sync engine has one server; the others wait in `workspaces.json` with
//! their member tokens sealed beside it, and the app switches between them
//! without a new invite.
//!
//! The sync engine syncs whatever has sync turned on to the active server, so
//! a switch parks what belongs to the server being left: every profile, proxy,
//! group, VPN, extension and extension group with sync on has it turned off
//! locally (no `updated_at` bump, nothing sent anywhere) and is listed in that
//! server's entry, and switching back turns it on again. Leaving a server for
//! good does the same and forgets the list, so its profiles stay on this
//! computer as local copies and never reach another server.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::profile::types::SyncMode;
use crate::profile::ProfileManager;
use crate::settings_manager::SettingsManager;
use crate::workspace::{WorkspaceInfo, WorkspaceMember};

const SAVED_FILE: &str = "workspaces.json";
const TOKENS_DIR: &str = "workspace-tokens";
const TOKEN_MAGIC: &[u8; 6] = b"OWLWS1";

/// What sync was turned off for when its server stopped being the active one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Parked {
  #[serde(default)]
  pub profiles: Vec<(String, SyncMode)>,
  #[serde(default)]
  pub proxies: Vec<String>,
  #[serde(default)]
  pub groups: Vec<String>,
  #[serde(default)]
  pub vpns: Vec<String>,
  #[serde(default)]
  pub extensions: Vec<String>,
  #[serde(default)]
  pub extension_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedServer {
  info: WorkspaceInfo,
  #[serde(default)]
  parked: Parked,
}

/// A joined server as the Server page lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SavedWorkspace {
  pub server: String,
  pub name: String,
  pub member: WorkspaceMember,
  pub active: bool,
}

fn error(code: &str) -> String {
  crate::backend_error(code)
}

fn saved_path() -> PathBuf {
  crate::app_dirs::data_dir().join(SAVED_FILE)
}

fn token_path(server: &str) -> PathBuf {
  let digest = Sha256::digest(server.as_bytes());
  let name: String = digest[..12].iter().map(|b| format!("{b:02x}")).collect();
  crate::app_dirs::data_dir()
    .join(TOKENS_DIR)
    .join(format!("{name}.token"))
}

fn read_saved() -> Vec<SavedServer> {
  std::fs::read_to_string(saved_path())
    .ok()
    .and_then(|text| serde_json::from_str(&text).ok())
    .unwrap_or_default()
}

fn write_saved(saved: &[SavedServer]) -> Result<(), String> {
  let path = saved_path();
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
  let tmp = path.with_extension("json.tmp");
  std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
  std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Keep the active server, and its token, in the list. Called after a join
/// and before anything reads the list, so an install that joined before the
/// list existed shows its server too.
pub async fn remember_active(app_handle: &tauri::AppHandle) -> Result<(), String> {
  let Some(info) = crate::workspace::current() else {
    return Ok(());
  };
  if let Some(token) = SettingsManager::instance()
    .get_sync_token(app_handle)
    .await
    .ok()
    .flatten()
  {
    crate::vault::seal(&token_path(&info.server), TOKEN_MAGIC, &token)?;
  }
  let mut saved = read_saved();
  match saved.iter_mut().find(|s| s.info.server == info.server) {
    Some(entry) => entry.info = info,
    None => saved.push(SavedServer {
      info,
      parked: Parked::default(),
    }),
  }
  write_saved(&saved)
}

/// A switch while a shared profile is open would hand its lock back while the
/// browser still writes to it, so it waits until those are closed.
pub(crate) fn refuse_while_shared_profiles_run() -> Result<(), String> {
  let running = ProfileManager::instance()
    .list_profiles()
    .unwrap_or_default()
    .into_iter()
    .any(|p| p.is_sync_enabled() && p.process_id.is_some());
  if running {
    return Err(error("WORKSPACE_PROFILES_RUNNING"));
  }
  Ok(())
}

/// Turn sync off locally for everything that has it on, and say what that was.
fn park_all() -> Parked {
  let mut parked = Parked::default();

  let profiles = ProfileManager::instance();
  for mut profile in profiles.list_profiles().unwrap_or_default() {
    if !profile.is_sync_enabled() {
      continue;
    }
    let id = profile.id.to_string();
    let mode = profile.sync_mode;
    profile.sync_mode = SyncMode::Disabled;
    match profiles.save_profile(&profile) {
      Ok(()) => parked.profiles.push((id, mode)),
      Err(e) => log::warn!("Could not park profile {id}: {e}"),
    }
  }

  let proxies = &crate::proxy_manager::PROXY_MANAGER;
  for proxy in proxies.get_stored_proxies() {
    if proxy.sync_enabled && !proxy.is_cloud_managed {
      match proxies.set_stored_proxy_sync_state(&proxy.id, false, proxy.last_sync) {
        Ok(_) => parked.proxies.push(proxy.id),
        Err(e) => log::warn!("Could not park proxy {}: {e}", proxy.id),
      }
    }
  }

  if let Ok(manager) = crate::group_manager::GROUP_MANAGER.lock() {
    for mut group in manager.get_all_groups().unwrap_or_default() {
      if group.sync_enabled {
        group.sync_enabled = false;
        if manager.update_group_internal(&group).is_ok() {
          parked.groups.push(group.id);
        }
      }
    }
  }

  if let Ok(storage) = crate::vpn::VPN_STORAGE.lock() {
    for config in storage.list_configs().unwrap_or_default() {
      if config.sync_enabled
        && storage
          .update_sync_fields(&config.id, false, config.last_sync)
          .is_ok()
      {
        parked.vpns.push(config.id);
      }
    }
  }

  if let Ok(manager) = crate::extension_manager::EXTENSION_MANAGER.lock() {
    for mut ext in manager.list_extensions().unwrap_or_default() {
      if ext.sync_enabled {
        ext.sync_enabled = false;
        if manager.update_extension_internal(&ext).is_ok() {
          parked.extensions.push(ext.id);
        }
      }
    }
    for mut group in manager.list_groups().unwrap_or_default() {
      if group.sync_enabled {
        group.sync_enabled = false;
        if manager.update_group_internal(&group).is_ok() {
          parked.extension_groups.push(group.id);
        }
      }
    }
  }

  parked
}

/// Turn sync back on for what a server's entry parked. Anything deleted
/// meanwhile is skipped.
fn unpark(parked: &Parked) {
  let profiles = ProfileManager::instance();
  let local = profiles.list_profiles().unwrap_or_default();
  for (id, mode) in &parked.profiles {
    if let Some(mut profile) = local.iter().find(|p| &p.id.to_string() == id).cloned() {
      profile.sync_mode = *mode;
      if let Err(e) = profiles.save_profile(&profile) {
        log::warn!("Could not unpark profile {id}: {e}");
      }
    }
  }

  let proxies = &crate::proxy_manager::PROXY_MANAGER;
  for proxy in proxies.get_stored_proxies() {
    if parked.proxies.contains(&proxy.id) {
      let _ = proxies.set_stored_proxy_sync_state(&proxy.id, true, proxy.last_sync);
    }
  }

  if let Ok(manager) = crate::group_manager::GROUP_MANAGER.lock() {
    for mut group in manager.get_all_groups().unwrap_or_default() {
      if parked.groups.contains(&group.id) {
        group.sync_enabled = true;
        let _ = manager.update_group_internal(&group);
      }
    }
  }

  if let Ok(storage) = crate::vpn::VPN_STORAGE.lock() {
    for config in storage.list_configs().unwrap_or_default() {
      if parked.vpns.contains(&config.id) {
        let _ = storage.update_sync_fields(&config.id, true, config.last_sync);
      }
    }
  }

  if let Ok(manager) = crate::extension_manager::EXTENSION_MANAGER.lock() {
    for mut ext in manager.list_extensions().unwrap_or_default() {
      if parked.extensions.contains(&ext.id) {
        ext.sync_enabled = true;
        let _ = manager.update_extension_internal(&ext);
      }
    }
    for mut group in manager.list_groups().unwrap_or_default() {
      if parked.extension_groups.contains(&group.id) {
        group.sync_enabled = true;
        let _ = manager.update_group_internal(&group);
      }
    }
  }
}

fn emit_entity_changes() {
  for event in [
    "profiles-changed",
    "stored-proxies-changed",
    "groups-changed",
    "vpn-configs-changed",
    "extensions-changed",
  ] {
    let _ = crate::events::emit(event, ());
  }
}

/// Stop syncing with the active server and park what belongs to it in its
/// entry. `forget` drops the entry and its token instead, leaving those as
/// local copies.
pub async fn deactivate(app_handle: &tauri::AppHandle, forget: bool) -> Result<(), String> {
  let Some(info) = crate::workspace::current() else {
    return Ok(());
  };
  refuse_while_shared_profiles_run()?;
  remember_active(app_handle).await?;
  crate::workspace::disconnect(app_handle).await?;

  let parked = park_all();
  let mut saved = read_saved();
  if forget {
    saved.retain(|s| s.info.server != info.server);
    let _ = std::fs::remove_file(token_path(&info.server));
  } else if let Some(entry) = saved.iter_mut().find(|s| s.info.server == info.server) {
    entry.parked = parked;
  }
  write_saved(&saved)?;
  emit_entity_changes();
  Ok(())
}

#[tauri::command]
pub async fn list_saved_workspaces(
  app_handle: tauri::AppHandle,
) -> Result<Vec<SavedWorkspace>, String> {
  remember_active(&app_handle).await?;
  let active = crate::workspace::current().map(|i| i.server);
  Ok(
    read_saved()
      .into_iter()
      .map(|s| SavedWorkspace {
        active: active.as_deref() == Some(s.info.server.as_str()),
        server: s.info.server,
        name: s.info.name,
        member: s.info.member,
      })
      .collect(),
  )
}

/// Make another joined server the active one.
#[tauri::command]
pub async fn switch_workspace(
  app_handle: tauri::AppHandle,
  server: String,
) -> Result<WorkspaceInfo, String> {
  if crate::workspace::current().is_some_and(|i| i.server == server) {
    return crate::workspace::current().ok_or_else(|| error("WORKSPACE_NOT_JOINED"));
  }
  let target = read_saved()
    .into_iter()
    .find(|s| s.info.server == server)
    .ok_or_else(|| error("WORKSPACE_SAVED_NOT_FOUND"))?;
  let token = crate::vault::open(&token_path(&server), TOKEN_MAGIC)?
    .ok_or_else(|| error("WORKSPACE_SAVED_NOT_FOUND"))?;

  deactivate(&app_handle, false).await?;
  unpark(&target.parked);
  let mut saved = read_saved();
  if let Some(entry) = saved.iter_mut().find(|s| s.info.server == server) {
    entry.parked = Parked::default();
  }
  write_saved(&saved)?;
  emit_entity_changes();

  crate::workspace::activate(&app_handle, target.info.clone(), &token).await?;
  log::info!("Switched to the Owl server {}", target.info.name);
  Ok(target.info)
}

/// Take a server that is not the active one off the list. Its profiles stay
/// on this computer as local copies.
#[tauri::command]
pub async fn forget_saved_workspace(server: String) -> Result<(), String> {
  if crate::workspace::current().is_some_and(|i| i.server == server) {
    return Err(error("WORKSPACE_SAVED_IS_ACTIVE"));
  }
  let mut saved = read_saved();
  saved.retain(|s| s.info.server != server);
  write_saved(&saved)?;
  let _ = std::fs::remove_file(token_path(&server));
  let _ = crate::events::emit("workspace-changed", ());
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn each_server_has_its_own_token_file() {
    let a = token_path("https://owl.example.com");
    let b = token_path("https://other.example.com");
    assert_ne!(a, b);
    assert_eq!(a, token_path("https://owl.example.com"));
    assert!(a.file_name().unwrap().to_string_lossy().ends_with(".token"));
  }

  #[test]
  fn an_entry_written_before_parking_existed_still_reads() {
    let text = r#"[{"info":{"server":"https://a","name":"A","member":{"id":"m1","name":"Ivan","role":"owner"}}}]"#;
    let saved: Vec<SavedServer> = serde_json::from_str(text).unwrap();
    assert_eq!(saved[0].parked, Parked::default());
    let parked = Parked {
      profiles: vec![("p1".into(), SyncMode::Encrypted)],
      ..Parked::default()
    };
    let round: Parked = serde_json::from_str(&serde_json::to_string(&parked).unwrap()).unwrap();
    assert_eq!(round, parked);
  }
}
