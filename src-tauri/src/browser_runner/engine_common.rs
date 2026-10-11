//! Bookkeeping every engine Antidetect adds shares around a launch and a stop.

use super::BrowserRunner;
use crate::events;
use crate::profile::BrowserProfile;
use serde::Serialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Serialize)]
struct RunningChangedPayload {
  id: String,
  is_running: bool,
}

fn emit_running_changed(profile: &BrowserProfile, is_running: bool) {
  let payload = RunningChangedPayload {
    id: profile.id.to_string(),
    is_running,
  };
  if let Err(e) = events::emit("profile-running-changed", &payload) {
    log::warn!("Warning: Failed to emit profile running changed event: {e}");
  }
}

/// Whether moving `profile` to `version` keeps it on the kind of build it is
/// on. A CloakBrowser profile never crosses between the free build and a Pro
/// one by itself: a profile a newer Chromium has opened cannot go back, and
/// the Pro build stops working when its licence does.
pub(crate) fn stays_in_tier(profile: &BrowserProfile, version: &str) -> bool {
  use crate::api_client::ApiClient;
  profile.browser != "cloakbrowser"
    || ApiClient::cloakbrowser_is_pro(version) == ApiClient::cloakbrowser_is_pro(&profile.version)
}

/// How long a launch waits for the location database a fresh install is still
/// downloading.
const LOCATION_DATABASE_WAIT: Duration = Duration::from_secs(150);

/// Make sure the database that turns an exit address into a timezone and a
/// language is there, starting or waiting out its download if it is not.
///
/// Without it a profile starts with this machine's own timezone and language
/// whatever its proxy, and nothing says so. A launch that depends on the
/// database therefore waits for it, and fails with a code when it does not
/// arrive, instead of going ahead unlocated.
pub(super) async fn require_location_database(
  app_handle: &tauri::AppHandle,
) -> Result<(), BoxError> {
  use crate::geoip_downloader::GeoIPDownloader;
  if GeoIPDownloader::is_geoip_database_available() {
    return Ok(());
  }
  log::info!("Waiting for the location database before launching");
  let arrived = tokio::time::timeout(LOCATION_DATABASE_WAIT, async {
    while !GeoIPDownloader::is_geoip_database_available() {
      // Returns at once when another caller is already downloading.
      if let Err(e) = GeoIPDownloader::instance()
        .download_geoip_database(app_handle)
        .await
      {
        log::warn!("Location database download failed: {e}");
      }
      tokio::time::sleep(Duration::from_secs(2)).await;
    }
  })
  .await;
  match arrived {
    Ok(()) => Ok(()),
    Err(_) => Err(crate::backend_error("LOCATION_DATABASE_UNAVAILABLE").into()),
  }
}

/// How long a launch waits to learn its own exit address before going ahead
/// with what the profile already says about its location.
const EXIT_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// The address sites will see this launch come from, measured through its
/// local proxy unless a launch gate has measured it already.
pub(super) async fn exit_ip(profile: &BrowserProfile, local_proxy_url: &str) -> Option<String> {
  if let Some(ip) = crate::fingerprint_consistency::cached_exit_ip(profile) {
    return Some(ip);
  }
  tokio::time::timeout(
    EXIT_PROBE_TIMEOUT,
    crate::ip_utils::fetch_public_ip(Some(local_proxy_url)),
  )
  .await
  .ok()
  .and_then(Result::ok)
}

pub(super) fn process_is_gone(pid: u32) -> bool {
  let system = sysinfo::System::new_all();
  system.process(sysinfo::Pid::from(pid as usize)).is_none()
}

/// Kill `pid` with the platform helper and report whether it is gone.
pub(super) async fn force_kill(pid: u32, profile_path: &str) -> bool {
  #[cfg(target_os = "macos")]
  let result =
    crate::platform_browser::macos::kill_browser_process_impl(pid, Some(profile_path)).await;
  #[cfg(target_os = "linux")]
  let result =
    crate::platform_browser::linux::kill_browser_process_impl(pid, Some(profile_path)).await;
  #[cfg(target_os = "windows")]
  let result = {
    let _ = profile_path;
    crate::platform_browser::windows::kill_browser_process_impl(pid).await
  };

  if let Err(e) = result {
    log::error!("Failed to force kill browser process {pid}: {e}");
    return false;
  }
  tokio::time::sleep(Duration::from_millis(500)).await;
  process_is_gone(pid)
}

impl BrowserRunner {
  /// Persist a successful launch and tell the UI the profile is running.
  pub(super) fn record_launch(
    &self,
    profile: &mut BrowserProfile,
    process_id: u32,
  ) -> Result<(), BoxError> {
    profile.process_id = Some(process_id);
    profile.last_launch = Some(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
    self.save_process_info(profile)?;
    let _ = crate::tag_manager::TAG_MANAGER.lock().map(|tm| {
      let _ = tm.rebuild_from_profiles(&self.profile_manager.list_profiles().unwrap_or_default());
    });

    if let Err(e) = events::emit_empty("profiles-changed") {
      log::warn!("Warning: Failed to emit profiles-changed event: {e}");
    }
    if let Err(e) = events::emit("profile-updated", &*profile) {
      log::warn!("Warning: Failed to emit profile update event: {e}");
    }
    emit_running_changed(profile, true);
    Ok(())
  }

  /// Everything that follows a browser process actually going away: clear the
  /// stored PID, apply a pending engine update, tell the UI, and seal or drop
  /// the profile's data dir.
  pub(super) async fn finish_stop(
    &self,
    app_handle: &tauri::AppHandle,
    profile: &BrowserProfile,
  ) -> Result<(), BoxError> {
    // Saved immediately so a following version update (which re-reads the
    // profile from disk) sees the cleared PID.
    let mut updated_profile = profile.clone();
    updated_profile.process_id = None;
    self
      .save_process_info(&updated_profile)
      .map_err(|e| format!("Failed to update profile: {e}"))?;

    if let Some(pending_update) = self
      .auto_updater
      .get_pending_update(&profile.browser, &profile.version)
      .ok()
      .flatten()
      .filter(|update| stays_in_tier(profile, &update.new_version))
    {
      match self.profile_manager.update_profile_version(
        app_handle,
        &profile.id.to_string(),
        &pending_update.new_version,
      ) {
        Ok(after_update) => {
          updated_profile = after_update;
          if let Err(e) = self
            .auto_updater
            .dismiss_update_notification(&pending_update.id)
          {
            log::warn!("Warning: Failed to dismiss pending update notification: {e}");
          }
        }
        Err(e) => log::error!(
          "Failed to apply pending update for profile {}: {e}",
          profile.name
        ),
      }
    }
    if updated_profile.version == profile.version {
      if let Some(p) = self
        .auto_updater
        .update_profile_to_latest_installed(app_handle, &updated_profile)
      {
        updated_profile = p;
      }
    }

    if let Err(e) = events::emit("profile-updated", &updated_profile) {
      log::warn!("Warning: Failed to emit profile update event: {e}");
    }
    emit_running_changed(&updated_profile, false);

    if profile.password_protected {
      crate::profile::password::complete_after_quit_and_wait(profile).await;
    }
    crate::profile::after_exit::settle(app_handle, profile).await;

    if let Ok(consolidated) = self
      .downloaded_browsers_registry
      .consolidate_browser_versions(app_handle)
    {
      for action in &consolidated {
        log::info!("Post-stop version consolidation: {action}");
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn profile(browser: &str, version: &str) -> BrowserProfile {
    BrowserProfile {
      browser: browser.to_string(),
      version: version.to_string(),
      ..Default::default()
    }
  }

  #[test]
  fn a_cloakbrowser_profile_never_crosses_between_the_free_and_a_pro_build() {
    let Some(free) = crate::api_client::ApiClient::cloakbrowser_version() else {
      return;
    };
    let pro = "154.0.8037.57.1";
    assert!(!stays_in_tier(&profile("cloakbrowser", free), pro));
    assert!(!stays_in_tier(&profile("cloakbrowser", pro), free));
    // Within a tier, and for every other engine, an update goes ahead.
    assert!(stays_in_tier(&profile("cloakbrowser", pro), "155.0.1.1"));
    assert!(stays_in_tier(&profile("cloakbrowser", free), free));
    assert!(stays_in_tier(
      &profile("camoufox", "v156.0.1-beta.36"),
      "v157.0-beta.1"
    ));
  }
}
