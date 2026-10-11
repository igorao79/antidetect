//! CloakBrowser (Chromium) launch, open-URL and stop paths of `BrowserRunner`.

use super::engine_common::{exit_ip, force_kill, process_is_gone, require_location_database};
use super::{emit_launch_stage, BrowserRunner};
use crate::cloakbrowser_manager::{self, ExitLocation, LaunchRequest};
use crate::profile::BrowserProfile;
use crate::proxy_manager::PROXY_MANAGER;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// How long a browser gets to exit after `Browser.close` before it is killed.
const GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_secs(8);

/// Where the route looks like it comes out: the address measured through the
/// local proxy, geolocated with the bundled database. Every field is best
/// effort; an unknown exit just leaves those switches to the browser.
async fn exit_location(profile: &BrowserProfile, local_proxy_url: &str) -> ExitLocation {
  let ip = exit_ip(profile, local_proxy_url).await;
  let geo = ip
    .as_deref()
    .and_then(|ip| crate::geolocation::get_profile_geolocation(ip).ok());
  ExitLocation {
    timezone: geo.as_ref().map(|g| g.timezone.clone()),
    locale: geo.as_ref().map(|g| g.locale.as_string()),
    ip,
  }
}

impl BrowserRunner {
  pub(super) async fn launch_cloakbrowser_internal(
    &self,
    app_handle: tauri::AppHandle,
    profile: &BrowserProfile,
    url: Option<String>,
    remote_debugging_port: Option<u16>,
    headless: bool,
    gate: &crate::launch_gate::FingerprintGate,
  ) -> Result<BrowserProfile, BoxError> {
    let mut updated_profile = profile.clone();
    let mut config = profile.cloakbrowser_config.clone().unwrap_or_default();
    if config.seed.is_none() || config.randomize_fingerprint_on_launch == Some(true) {
      config.seed = Some(cloakbrowser_manager::generate_seed());
      updated_profile.cloakbrowser_config = Some(config.clone());
    }

    let executable = self.get_browser_executable_path(profile)?;

    // A timezone or language left to follow the exit comes from the database.
    if config.timezone.is_none() || config.locale.is_none() {
      require_location_database(&app_handle).await?;
    }

    emit_launch_stage(profile, "network", None);
    // SOCKS5 lets Chromium send UDP (QUIC, WebRTC) and name lookups through
    // the proxy instead of around it.
    let mut network = self
      .prepare_launch_network(&app_handle, profile, gate, "socks5")
      .await?;
    let proxy_url = format!(
      "socks5://{}:{}",
      network.local_proxy.host, network.local_proxy.port
    );
    let proxied = profile.proxy_id.is_some() || profile.vpn_id.is_some();
    // Looked up for a direct connection too: the timezone and language must
    // match the address sites see, and a system-wide VPN moves that address
    // without the profile knowing.
    let exit = exit_location(profile, &proxy_url).await;

    if profile.password_protected {
      crate::profile::password::prepare_for_launch(profile)
        .map_err(|e| -> BoxError { e.into() })?;
    } else if profile.ephemeral {
      crate::ephemeral_dirs::create_ephemeral_dir(&profile.id.to_string())
        .map_err(|e| -> BoxError { e.into() })?;
    }
    let profiles_dir = self.profile_manager.get_profiles_dir();
    let profile_data_path =
      crate::ephemeral_dirs::get_effective_profile_path(&updated_profile, &profiles_dir);
    std::fs::create_dir_all(&profile_data_path)?;
    let profile_path = profile_data_path.to_string_lossy().to_string();

    emit_launch_stage(profile, "extensions", None);
    let mut extension_paths = Vec::new();
    if updated_profile.extension_group_id.is_some() {
      let mgr = crate::extension_manager::EXTENSION_MANAGER.lock().unwrap();
      match mgr.install_extensions_for_profile(&updated_profile, &profile_data_path) {
        Ok(paths) => extension_paths = paths,
        Err(e) => log::warn!("Failed to install extensions for CloakBrowser profile: {e}"),
      }
    }

    let port = match remote_debugging_port {
      Some(port) => port,
      None => {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        listener.local_addr()?.port()
      }
    };

    let camera_dir = config
      .camera_video
      .is_some()
      .then(|| crate::cloakbrowser_camera::camera_dir(&profiles_dir, &profile.id.to_string()));

    emit_launch_stage(profile, "starting", None);
    let process = cloakbrowser_manager::launch(LaunchRequest {
      executable: &executable,
      profile_path: &profile_path,
      profile_id: &profile.id.to_string(),
      fingerprint_args: cloakbrowser_manager::fingerprint_args(&config, &exit, proxied),
      proxy_url: Some(&proxy_url),
      url: url.as_deref(),
      remote_debugging_port: port,
      headless,
      ephemeral: profile.ephemeral,
      extension_paths: &extension_paths,
      camera_dir: camera_dir.as_deref(),
    })
    .await
    .map_err(|e| -> BoxError {
      crate::wrap_backend_error(e, "Failed to launch CloakBrowser").into()
    })?;
    log::info!("CloakBrowser launched with PID: {}", process.pid);

    if let Err(error) = network.commit(&updated_profile.id.to_string(), process.pid) {
      force_kill(process.pid, &profile_path).await;
      return Err(error);
    }

    self.record_launch(&mut updated_profile, process.pid)?;
    Ok(updated_profile)
  }

  pub(super) async fn open_url_in_cloakbrowser(
    &self,
    profile: &BrowserProfile,
    url: &str,
  ) -> Result<(), BoxError> {
    let profiles_dir = self.profile_manager.get_profiles_dir();
    let profile_path = crate::ephemeral_dirs::get_effective_profile_path(profile, &profiles_dir);
    let process = cloakbrowser_manager::find_by_profile(&profile_path.to_string_lossy())
      .ok_or("CloakBrowser is not running")?;
    let port = process
      .cdp_port
      .ok_or("CloakBrowser was started without a DevTools port")?;
    cloakbrowser_manager::open_url(port, url)
      .await
      .map_err(Into::into)
  }

  pub(super) async fn kill_cloakbrowser_process(
    &self,
    app_handle: tauri::AppHandle,
    profile: &BrowserProfile,
  ) -> Result<(), BoxError> {
    let profiles_dir = self.profile_manager.get_profiles_dir();
    let profile_path = crate::ephemeral_dirs::get_effective_profile_path(profile, &profiles_dir)
      .to_string_lossy()
      .to_string();

    let profile_id = profile.id.to_string();
    if let Err(e) = PROXY_MANAGER
      .stop_proxy_by_profile_id(app_handle.clone(), &profile_id)
      .await
    {
      log::warn!("Warning: Failed to stop proxy for profile {profile_id}: {e}");
    }

    if let Some(process) = cloakbrowser_manager::find_by_profile(&profile_path) {
      log::info!(
        "Stopping CloakBrowser PID {} for profile {}",
        process.pid,
        profile.name
      );
      // Ask first: a browser that exits on its own has written its cookies.
      let mut exited = false;
      if let Some(port) = process.cdp_port {
        match cloakbrowser_manager::close_gracefully(port).await {
          Ok(()) => {
            for _ in 0..(GRACEFUL_EXIT_TIMEOUT.as_millis() / 250) {
              if process_is_gone(process.pid) {
                exited = true;
                break;
              }
              tokio::time::sleep(Duration::from_millis(250)).await;
            }
          }
          Err(e) => log::warn!("CloakBrowser did not take Browser.close: {e}"),
        }
      }
      if !exited {
        log::warn!(
          "CloakBrowser PID {} did not exit on its own; terminating it",
          process.pid
        );
        if !force_kill(process.pid, &profile_path).await && !process_is_gone(process.pid) {
          return Err(
            format!(
              "Failed to stop CloakBrowser for profile {} - process may still be running",
              profile.name
            )
            .into(),
          );
        }
      }
    }

    self.finish_stop(&app_handle, profile).await
  }
}
