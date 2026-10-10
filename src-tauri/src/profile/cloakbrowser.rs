//! CloakBrowser-specific profile operations.

use super::{BrowserProfile, ProfileManager};
use crate::cloakbrowser_manager::{self, CloakBrowserConfig};
use crate::events;

/// The config a new CloakBrowser profile starts with: its own seed, presenting
/// as the host OS. `None` for any other engine.
pub fn config_for_new_profile(browser: &str) -> Option<CloakBrowserConfig> {
  (browser == "cloakbrowser").then(|| CloakBrowserConfig {
    seed: Some(cloakbrowser_manager::generate_seed()),
    os: Some(crate::profile::types::get_host_os()),
    ..Default::default()
  })
}

fn load_profile(manager: &ProfileManager, profile_id: &str) -> Result<BrowserProfile, String> {
  let id =
    uuid::Uuid::parse_str(profile_id).map_err(|_| crate::backend_error("INVALID_PROFILE_ID"))?;
  manager
    .list_profiles()
    .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?
    .into_iter()
    .find(|p| p.id == id)
    .ok_or_else(|| crate::backend_error("PROFILE_NOT_FOUND"))
}

impl ProfileManager {
  /// Replace a CloakBrowser profile's config. A config without a seed keeps the
  /// profile's current one, so editing a single field never changes the device.
  pub async fn update_cloakbrowser_config(
    &self,
    app_handle: tauri::AppHandle,
    profile_id: &str,
    mut config: CloakBrowserConfig,
  ) -> Result<BrowserProfile, String> {
    let mut profile = load_profile(self, profile_id)?;
    if self
      .check_browser_status(app_handle, &profile)
      .await
      .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?
    {
      return Err(crate::backend_error("PROFILE_RUNNING"));
    }

    if config.seed.is_none() {
      config.seed = profile
        .cloakbrowser_config
        .as_ref()
        .and_then(|c| c.seed)
        .or_else(|| Some(cloakbrowser_manager::generate_seed()));
    }
    profile.cloakbrowser_config = Some(config);
    profile.updated_at = Some(crate::proxy_manager::now_secs());
    self
      .save_profile(&profile)
      .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?;
    crate::sync::queue_profile_sync_if_eligible(&profile);

    if let Err(e) = events::emit_empty("profiles-changed") {
      log::warn!("Failed to emit profiles-changed event: {e}");
    }
    Ok(profile)
  }

  /// Whether the profile's CloakBrowser is running, keeping the stored PID in
  /// step with what is actually there.
  pub(super) async fn check_cloakbrowser_status(
    &self,
    app_handle: &tauri::AppHandle,
    profile: &BrowserProfile,
  ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let profile_path =
      crate::ephemeral_dirs::get_effective_profile_path(profile, &self.get_profiles_dir());
    let running = cloakbrowser_manager::find_by_profile(&profile_path.to_string_lossy());
    let running_pid = running.as_ref().map(|process| process.pid);

    // Re-read so a concurrent metadata edit is not overwritten by this snapshot.
    let Ok(mut latest) = load_profile(self, &profile.id.to_string()) else {
      return Ok(running.is_some());
    };
    if latest.process_id != running_pid {
      let previous = latest.process_id;
      latest.process_id = running_pid;
      if let Err(e) = self.save_profile(&latest) {
        log::warn!("Failed to update CloakBrowser process info: {e}");
      }
      match (previous, running_pid) {
        (Some(prev), Some(new)) => {
          let _ = crate::proxy_manager::PROXY_MANAGER.update_proxy_pid(prev, new);
        }
        (Some(_), None) => {
          if profile.ephemeral {
            crate::ephemeral_dirs::remove_ephemeral_dir(&profile.id.to_string());
          }
          if let Some(updated) = crate::auto_updater::AutoUpdater::instance()
            .update_profile_to_latest_installed(app_handle, &latest)
          {
            latest = updated;
          }
        }
        _ => {}
      }
      if let Err(e) = events::emit("profile-updated", &latest) {
        log::warn!("Failed to emit profile update event: {e}");
      }
    }
    Ok(running.is_some())
  }
}

#[tauri::command]
pub async fn update_cloakbrowser_config(
  app_handle: tauri::AppHandle,
  profile_id: String,
  config: CloakBrowserConfig,
) -> Result<(), String> {
  ProfileManager::instance()
    .update_cloakbrowser_config(app_handle, &profile_id, config)
    .await
    .map(|_| ())
}
