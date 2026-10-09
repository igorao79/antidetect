//! Camoufox (Firefox) launch, open-URL and stop paths of `BrowserRunner`.
//!
//! Restored from upstream Donut (removed in 23859333c6) and adapted to the
//! current launch pipeline via [`super::launch_network`].

use super::{emit_launch_stage, BrowserRunner};
use crate::camoufox_manager::{CamoufoxConfig, CamoufoxManager};
use crate::events;
use crate::profile::BrowserProfile;
use crate::proxy_manager::PROXY_MANAGER;
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

fn process_is_gone(pid: u32) -> bool {
  let system = sysinfo::System::new_all();
  system.process(sysinfo::Pid::from(pid as usize)).is_none()
}

/// Kill `pid` with the platform helper and report whether it is gone.
async fn force_kill(pid: u32, profile_path: &str) -> bool {
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
    log::error!("Failed to force kill Camoufox process {pid}: {e}");
    return false;
  }
  tokio::time::sleep(Duration::from_millis(500)).await;
  process_is_gone(pid)
}

impl BrowserRunner {
  pub(super) async fn launch_camoufox_internal(
    &self,
    app_handle: tauri::AppHandle,
    profile: &BrowserProfile,
    url: Option<String>,
    remote_debugging_port: Option<u16>,
    headless: bool,
    gate: &crate::launch_gate::FingerprintGate,
  ) -> Result<BrowserProfile, BoxError> {
    let camoufox_manager = CamoufoxManager::instance();
    let mut camoufox_config = profile.camoufox_config.clone().unwrap_or_else(|| {
      log::info!(
        "No camoufox config found for profile {}, using default",
        profile.name
      );
      CamoufoxConfig::default()
    });

    emit_launch_stage(profile, "network", None);
    // Firefox's QUIC stack bypasses a configured proxy, so Camoufox keeps the
    // local HTTP proxy (QUIC is disabled in user.js) and HTTP CONNECT carries
    // everything.
    let mut network = self
      .prepare_launch_network(&app_handle, profile, gate, "http")
      .await?;

    camoufox_config.proxy = Some(format!(
      "http://{}:{}",
      network.local_proxy.host, network.local_proxy.port
    ));
    // Geolocation spoofing is derived from the proxy exit unless the user set
    // it explicitly.
    if camoufox_config.geoip.is_none() {
      camoufox_config.geoip = Some(serde_json::Value::Bool(true));
    }

    let mut updated_profile = profile.clone();
    let randomize = camoufox_config.randomize_fingerprint_on_launch == Some(true);
    if randomize || camoufox_config.fingerprint.is_none() {
      log::info!(
        "Generating {} fingerprint for Camoufox profile: {}",
        if randomize { "a random" } else { "a missing" },
        profile.name
      );
      let mut config_for_generation = camoufox_config.clone();
      config_for_generation.fingerprint = None;
      let new_fingerprint = camoufox_manager
        .generate_fingerprint_config(&app_handle, profile, &config_for_generation)
        .await
        .map_err(|e| format!("Failed to generate Camoufox fingerprint: {e}"))?;

      camoufox_config.fingerprint = Some(new_fingerprint.clone());

      // Persist only the fingerprint; the stored config keeps the user's own
      // proxy/geoip choices, not this launch's local proxy.
      let mut stored = updated_profile.camoufox_config.clone().unwrap_or_default();
      stored.fingerprint = Some(new_fingerprint);
      stored.randomize_fingerprint_on_launch = camoufox_config.randomize_fingerprint_on_launch;
      if camoufox_config.os.is_some() {
        stored.os = camoufox_config.os.clone();
      }
      updated_profile.camoufox_config = Some(stored);
    }

    let override_profile_path = if profile.password_protected {
      Some(
        crate::profile::password::prepare_for_launch(profile)
          .map_err(|e| -> BoxError { e.into() })?,
      )
    } else if profile.ephemeral {
      Some(
        crate::ephemeral_dirs::create_ephemeral_dir(&profile.id.to_string())
          .map_err(|e| -> BoxError { e.into() })?,
      )
    } else {
      None
    };

    emit_launch_stage(profile, "extensions", None);
    if updated_profile.extension_group_id.is_some() {
      let profiles_dir = self.profile_manager.get_profiles_dir();
      let ext_profile_path = override_profile_path
        .clone()
        .unwrap_or_else(|| updated_profile.get_profile_data_path(&profiles_dir));
      let mgr = crate::extension_manager::EXTENSION_MANAGER.lock().unwrap();
      match mgr.install_extensions_for_profile(&updated_profile, &ext_profile_path) {
        Ok(paths) if !paths.is_empty() => log::info!(
          "Installed {} Firefox extensions for profile: {}",
          paths.len(),
          updated_profile.name
        ),
        Ok(_) => {}
        Err(e) => log::warn!("Failed to install extensions for Camoufox profile: {e}"),
      }
    }

    emit_launch_stage(profile, "starting", None);
    log::info!("Launching Camoufox for profile: {}", profile.name);
    let camoufox_result = camoufox_manager
      .launch_camoufox_profile(
        app_handle.clone(),
        updated_profile.clone(),
        camoufox_config,
        url,
        override_profile_path,
        remote_debugging_port,
        headless,
      )
      .await
      .map_err(|e| -> BoxError { format!("Failed to launch Camoufox: {e}").into() })?;

    let Some(process_id) = camoufox_result.processId.filter(|pid| *pid != 0) else {
      let _ = camoufox_manager
        .stop_camoufox(&app_handle, &camoufox_result.id)
        .await;
      return Err(
        crate::backend_error_with_detail(
          "INTERNAL_ERROR",
          "Camoufox did not report a process identifier",
        )
        .into(),
      );
    };
    log::info!("Camoufox launched successfully with PID: {process_id}");

    if let Err(error) = network.commit(&updated_profile.id.to_string(), process_id) {
      let _ = camoufox_manager
        .stop_camoufox(&app_handle, &camoufox_result.id)
        .await;
      return Err(error);
    }

    updated_profile.process_id = Some(process_id);
    updated_profile.last_launch = Some(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
    self.save_process_info(&updated_profile)?;
    let _ = crate::tag_manager::TAG_MANAGER.lock().map(|tm| {
      let _ = tm.rebuild_from_profiles(&self.profile_manager.list_profiles().unwrap_or_default());
    });

    if let Err(e) = events::emit_empty("profiles-changed") {
      log::warn!("Warning: Failed to emit profiles-changed event: {e}");
    }
    if let Err(e) = events::emit("profile-updated", &updated_profile) {
      log::warn!("Warning: Failed to emit profile update event: {e}");
    }
    emit_running_changed(&updated_profile, true);

    Ok(updated_profile)
  }

  pub(super) async fn open_url_in_camoufox(
    &self,
    profile: &BrowserProfile,
    url: &str,
  ) -> Result<(), BoxError> {
    let profiles_dir = self.profile_manager.get_profiles_dir();
    let profile_data_path =
      crate::ephemeral_dirs::get_effective_profile_path(profile, &profiles_dir);
    let profile_path_str = profile_data_path.to_string_lossy();

    match CamoufoxManager::instance()
      .find_camoufox_by_profile(&profile_path_str)
      .await
    {
      Ok(Some(_)) => {
        let executable_path = self
          .get_browser_executable_path(profile)
          .map_err(|e| format!("Failed to get Camoufox executable path: {e}"))?;
        // Firefox remoting: a second invocation on the same profile hands the
        // URL to the running instance.
        let output = std::process::Command::new(&executable_path)
          .arg("-profile")
          .arg(&*profile_path_str)
          .arg("-new-tab")
          .arg(url)
          .output()
          .map_err(|e| format!("Failed to execute Camoufox: {e}"))?;
        if output.status.success() {
          Ok(())
        } else {
          let stderr = String::from_utf8_lossy(&output.stderr);
          Err(format!("Failed to open URL in existing Camoufox instance: {stderr}").into())
        }
      }
      Ok(None) => Err("Camoufox browser is not running".into()),
      Err(e) => Err(format!("Error checking Camoufox process: {e}").into()),
    }
  }

  pub(super) async fn kill_camoufox_process(
    &self,
    app_handle: tauri::AppHandle,
    profile: &BrowserProfile,
  ) -> Result<(), BoxError> {
    let camoufox_manager = CamoufoxManager::instance();
    let profiles_dir = self.profile_manager.get_profiles_dir();
    let profile_data_path =
      crate::ephemeral_dirs::get_effective_profile_path(profile, &profiles_dir);
    let profile_path_str = profile_data_path.to_string_lossy().to_string();

    log::info!(
      "Attempting to kill Camoufox process for profile: {} (ID: {})",
      profile.name,
      profile.id
    );

    let profile_id_str = profile.id.to_string();
    if let Err(e) = PROXY_MANAGER
      .stop_proxy_by_profile_id(app_handle.clone(), &profile_id_str)
      .await
    {
      log::warn!("Warning: Failed to stop proxy for profile {profile_id_str}: {e}");
    }

    let stopped = match camoufox_manager
      .find_camoufox_by_profile(&profile_path_str)
      .await
    {
      Ok(Some(instance)) => {
        let stop_result = camoufox_manager
          .stop_camoufox(&app_handle, &instance.id)
          .await;
        match (instance.processId, stop_result) {
          (Some(pid), Ok(true)) => {
            tokio::time::sleep(Duration::from_millis(500)).await;
            process_is_gone(pid) || force_kill(pid, &profile_path_str).await
          }
          (Some(pid), Ok(false) | Err(_)) => force_kill(pid, &profile_path_str).await,
          (None, Ok(stopped)) => stopped,
          (None, Err(e)) => {
            log::error!("Error stopping Camoufox process {}: {e}", instance.id);
            false
          }
        }
      }
      Ok(None) => true,
      Err(e) => {
        log::error!(
          "Error finding Camoufox process for profile {}: {e}",
          profile.name
        );
        false
      }
    };

    if !stopped {
      return Err(
        format!(
          "Failed to stop Camoufox process for profile {} - process may still be running",
          profile.name
        )
        .into(),
      );
    }

    // Saved immediately so a following version update (which re-reads the
    // profile from disk) sees the cleared PID.
    let mut updated_profile = profile.clone();
    updated_profile.process_id = None;
    self
      .save_process_info(&updated_profile)
      .map_err(|e| format!("Failed to update profile: {e}"))?;

    if let Ok(Some(pending_update)) = self
      .auto_updater
      .get_pending_update(&profile.browser, &profile.version)
    {
      match self.profile_manager.update_profile_version(
        &app_handle,
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
          "Failed to apply pending update for Camoufox profile {}: {e}",
          profile.name
        ),
      }
    }
    if updated_profile.version == profile.version {
      if let Some(p) = self
        .auto_updater
        .update_profile_to_latest_installed(&app_handle, &updated_profile)
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
    } else if profile.ephemeral {
      crate::ephemeral_dirs::remove_ephemeral_dir(&profile.id.to_string());
    }

    if let Ok(consolidated) = self
      .downloaded_browsers_registry
      .consolidate_browser_versions(&app_handle)
    {
      for action in &consolidated {
        log::info!("Post-stop version consolidation: {action}");
      }
    }

    Ok(())
  }
}
