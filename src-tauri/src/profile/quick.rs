//! Quick profiles (fork): a temporary Camoufox profile made and launched in one
//! click. It uses the `temporary` flag of the local API's one-time profiles,
//! but keeps its data on disk (`ephemeral` stays off), and when its browser
//! stops it is not deleted: the app asks whether to keep it.

use serde::Serialize;

use crate::camoufox_manager::CamoufoxConfig;
use crate::profile::{BrowserProfile, ProfileManager};

const QUICK_NAME_PREFIX: &str = "Quick";

#[derive(Clone, Serialize)]
pub struct QuickProfileStopped {
  pub id: String,
  pub name: String,
}

/// A quick profile is temporary with its data on disk; the API's one-time and
/// MCP profiles are temporary and ephemeral.
pub fn is_quick(profile: &BrowserProfile) -> bool {
  profile.temporary && !profile.ephemeral
}

/// Tell the app a quick profile's browser has stopped, so it can ask.
pub fn announce_stopped(profile: &BrowserProfile) {
  let payload = QuickProfileStopped {
    id: profile.id.to_string(),
    name: profile.name.clone(),
  };
  if let Err(e) = crate::events::emit("quick-profile-stopped", payload) {
    log::warn!("Event emit failed event=quick-profile-stopped err=\"{e}\"");
  }
}

/// `Quick 1`, `Quick 2`, ...: the first number no profile uses.
pub fn quick_name(existing: &[String]) -> String {
  let taken: std::collections::HashSet<String> =
    existing.iter().map(|name| name.to_lowercase()).collect();
  (1u32..)
    .map(|n| format!("{QUICK_NAME_PREFIX} {n}"))
    .find(|name| !taken.contains(&name.to_lowercase()))
    .unwrap_or_else(|| QUICK_NAME_PREFIX.to_string())
}

fn quick_profile(profile_id: &str) -> Result<BrowserProfile, String> {
  let profile = ProfileManager::instance()
    .find_profile(profile_id)
    .map_err(|e| crate::coded_failure("PROFILE_NOT_FOUND", e))?;
  if !is_quick(&profile) {
    return Err(crate::backend_error("PROFILE_NOT_FOUND"));
  }
  Ok(profile)
}

/// Make a temporary Camoufox profile with the default settings. The caller
/// launches it; a launch that fails is followed by `discard_quick_profile`.
#[tauri::command]
pub async fn create_quick_profile(
  app_handle: tauri::AppHandle,
  proxy_id: Option<String>,
  vpn_id: Option<String>,
) -> Result<BrowserProfile, String> {
  let mut versions = crate::downloaded_browsers_registry::DownloadedBrowsersRegistry::instance()
    .get_downloaded_versions("camoufox");
  versions.sort_by(|a, b| crate::api_client::compare_versions(b, a));
  let version = versions.into_iter().next().ok_or_else(|| {
    serde_json::json!({ "code": "BROWSER_NOT_DOWNLOADED", "params": { "browser": "Camoufox" } })
      .to_string()
  })?;

  let manager = ProfileManager::instance();
  let existing: Vec<String> = manager
    .list_profiles()
    .map_err(|e| crate::wrap_backend_error(e, "Failed to list profiles"))?
    .into_iter()
    .map(|profile| profile.name)
    .collect();
  let created = manager
    .create_profile_with_group(
      &app_handle,
      &quick_name(&existing),
      "camoufox",
      &version,
      "stable",
      proxy_id.filter(|id| !id.is_empty()),
      vpn_id.filter(|id| !id.is_empty()),
      Some(CamoufoxConfig::default()),
      None,
      None,
      false,
      None,
      None,
      None,
    )
    .await
    .map_err(|e| crate::wrap_backend_error(e, "Failed to create profile"))?;

  let mut profile = created;
  profile.temporary = true;
  manager
    .save_profile(&profile)
    .map_err(|e| crate::wrap_backend_error(e, "Failed to save profile"))?;
  if let Err(e) = crate::events::emit_empty("profiles-changed") {
    log::warn!("Event emit failed event=profiles-changed err=\"{e}\"");
  }
  log::info!("Quick profile created profile={}", profile.id);
  Ok(profile)
}

/// Turn a stopped quick profile into a regular one with `name`.
#[tauri::command]
pub async fn keep_quick_profile(
  app_handle: tauri::AppHandle,
  profile_id: String,
  name: String,
) -> Result<BrowserProfile, String> {
  quick_profile(&profile_id)?;
  let manager = ProfileManager::instance();
  // Renaming first: a name that is taken leaves the profile quick, so the
  // prompt can ask again.
  let mut profile = manager
    .rename_profile(&app_handle, &profile_id, &name)
    .map_err(|e| crate::coded_failure("INTERNAL_ERROR", e))?;
  profile.temporary = false;
  profile.updated_at = Some(crate::proxy_manager::now_secs());
  manager
    .save_profile(&profile)
    .map_err(|e| crate::wrap_backend_error(e, "Failed to save profile"))?;
  if let Err(e) = crate::events::emit_empty("profiles-changed") {
    log::warn!("Event emit failed event=profiles-changed err=\"{e}\"");
  }
  log::info!("Quick profile kept profile={profile_id}");
  Ok(profile)
}

/// Delete a quick profile for good.
#[tauri::command]
pub async fn discard_quick_profile(
  app_handle: tauri::AppHandle,
  profile_id: String,
) -> Result<(), String> {
  quick_profile(&profile_id)?;
  ProfileManager::instance()
    .delete_profile_permanently(&app_handle, &profile_id)
    .map_err(|e| crate::coded_failure("INTERNAL_ERROR", e))?;
  log::info!("Quick profile discarded profile={profile_id}");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_name_is_the_first_unused_number() {
    assert_eq!(quick_name(&[]), "Quick 1");
    let existing = vec!["quick 1".to_string(), "Quick 3".to_string()];
    assert_eq!(quick_name(&existing), "Quick 2");
  }
}
