//! Routing for a profile launch, shared by the engines Antidetect adds on top
//! of Wayfern (Camoufox, CloakBrowser).
//!
//! It runs the same steps, in the same order, as the Wayfern branch of
//! `launch_browser_internal`: Xray / VPN workers, the route-required and
//! fingerprint gates, the DNS blocklist, the launch hook, and finally the local
//! donut-proxy worker the browser talks to. Wayfern keeps its own inline copy so
//! upstream changes to it merge cleanly.

use super::BrowserRunner;
use crate::browser::ProxySettings;
use crate::profile::BrowserProfile;
use crate::proxy_manager::PROXY_MANAGER;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The local proxy a launch routes through, plus every worker it started for
/// that. Dropping it before [`LaunchNetwork::commit`] tears those workers down,
/// so a launch that fails half way never leaves a detached worker behind.
pub(crate) struct LaunchNetwork {
  /// The local donut-proxy endpoint the browser must use.
  pub local_proxy: ProxySettings,
  app_handle: tauri::AppHandle,
  profile_name: String,
  routing_pid: u32,
  proxy_armed: bool,
  xray_worker_id: Option<String>,
  vpn_worker: Option<StartedVpn>,
  /// This launch's hold on the VPN worker; released when the launch ends so a
  /// sibling launch cannot stop the worker in between.
  _vpn_claim: Option<crate::vpn_worker_runner::VpnLaunchClaim>,
}

struct StartedVpn {
  worker_id: String,
  vpn_id: String,
  created: bool,
}

impl LaunchNetwork {
  /// Bind every routing worker to the browser that is now running. After this
  /// the workers outlive the launch and reap themselves when the browser exits.
  pub fn commit(&mut self, profile_id: &str, browser_pid: u32) -> Result<(), BoxError> {
    PROXY_MANAGER
      .update_proxy_pid(self.routing_pid, browser_pid)
      .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?;
    self.routing_pid = browser_pid;
    if !PROXY_MANAGER.set_browser_pid_for_profile(profile_id, browser_pid) {
      return Err(crate::backend_error("INTERNAL_ERROR").into());
    }
    if let Some(worker_id) = self.xray_worker_id.as_deref() {
      if !crate::xray_worker_runner::set_browser_pid(worker_id, browser_pid) {
        return Err(crate::backend_error("XRAY_START_FAILED").into());
      }
    }
    self.proxy_armed = false;
    self.xray_worker_id = None;
    self.vpn_worker = None;
    Ok(())
  }
}

impl Drop for LaunchNetwork {
  fn drop(&mut self) {
    if self.proxy_armed {
      log::warn!(
        "Launch failed after local proxy start for profile {}; stopping proxy worker",
        self.profile_name
      );
      let app_handle = self.app_handle.clone();
      let pid = self.routing_pid;
      tauri::async_runtime::spawn(async move {
        if let Err(e) = PROXY_MANAGER.stop_proxy(app_handle, pid).await {
          log::warn!("Failed to stop proxy worker after failed launch: {e}");
        }
      });
    }
    if let Some(worker_id) = self.xray_worker_id.take() {
      log::warn!(
        "Launch failed after Xray-core start for profile {}; stopping worker",
        self.profile_name
      );
      if let Err(error) = crate::xray_worker_runner::stop_xray_worker_now(&worker_id) {
        log::warn!("Failed to stop Xray-core worker after failed launch: {error}");
      }
    }
    // Released before the in-use check, or this launch would count itself as a
    // reason to keep the worker it just failed to use.
    drop(self._vpn_claim.take());
    if let Some(vpn) = self.vpn_worker.take() {
      // A reused worker belongs to another running profile; only stop one this
      // launch created.
      if vpn.created {
        log::warn!(
          "Launch failed after VPN worker start for profile {}; stopping worker",
          self.profile_name
        );
        tauri::async_runtime::spawn(async move {
          let _adopt_guard = crate::vpn_worker_runner::lock_vpn_starts().await;
          if crate::vpn_worker_runner::vpn_id_in_use_by_running_browser(&vpn.vpn_id) {
            return;
          }
          if let Err(error) = crate::vpn_worker_runner::stop_vpn_worker(&vpn.worker_id).await {
            log::warn!("Failed to stop VPN worker after failed launch: {error}");
          }
        });
      }
    }
  }
}

impl BrowserRunner {
  /// Resolve the profile's upstream, pass the launch gates, and start the local
  /// proxy worker. `local_protocol` is what the worker serves the browser:
  /// `"socks5"` for Chromium engines (so QUIC and WebRTC UDP are proxied),
  /// `"http"` for Firefox engines (whose QUIC stack bypasses a SOCKS proxy).
  pub(crate) async fn prepare_launch_network(
    &self,
    app_handle: &tauri::AppHandle,
    profile: &BrowserProfile,
    gate: &crate::launch_gate::FingerprintGate,
    local_protocol: &str,
  ) -> Result<LaunchNetwork, BoxError> {
    let engine = crate::browser::BrowserType::from_str(&profile.browser)
      .map(|b| b.display_name())
      .unwrap_or("browser");

    let mut upstream_proxy = self
      .resolve_launch_proxy(profile)
      .await
      .map_err(|e| -> BoxError { e.into() })?;

    let mut network = LaunchNetwork {
      local_proxy: ProxySettings {
        proxy_type: local_protocol.to_string(),
        host: String::new(),
        port: 0,
        username: None,
        password: None,
        vless_uri: None,
      },
      app_handle: app_handle.clone(),
      profile_name: profile.name.clone(),
      routing_pid: 0,
      proxy_armed: false,
      xray_worker_id: None,
      vpn_worker: None,
      _vpn_claim: None,
    };

    if upstream_proxy
      .as_ref()
      .is_some_and(|proxy| crate::xray::is_xray_proxy_type(&proxy.proxy_type))
    {
      let vless_uri = upstream_proxy
        .as_ref()
        .and_then(|proxy| proxy.vless_uri.as_deref())
        .ok_or_else(|| crate::backend_error("VLESS_CONFIG_INVALID"))?;
      let worker =
        crate::xray_worker_runner::start_xray_worker(Some(&profile.id.to_string()), vless_uri)
          .await
          .map_err(|error| -> BoxError { error.to_string().into() })?;
      log::info!(
        "Xray-core worker started for {engine} profile on port {}",
        worker.local_port
      );
      network.xray_worker_id = Some(worker.id.clone());
      upstream_proxy = Some(worker.local_proxy_settings());
    }

    if upstream_proxy.is_none() {
      if let Some(ref vpn_id) = profile.vpn_id {
        let started = crate::vpn_worker_runner::start_vpn_worker_tracked(vpn_id)
          .await
          .map_err(|e| -> BoxError {
            crate::backend_error_with_detail("VPN_WORKER_START_FAILED", e).into()
          })?;
        network.vpn_worker = Some(StartedVpn {
          worker_id: started.config.id.clone(),
          vpn_id: vpn_id.clone(),
          created: started.created,
        });
        network._vpn_claim = Some(started.claim);
        if let Some(port) = started.config.local_port {
          upstream_proxy = Some(ProxySettings {
            proxy_type: "socks5".to_string(),
            host: "127.0.0.1".to_string(),
            port,
            username: None,
            password: None,
            vless_uri: None,
          });
          log::info!("VPN worker started for {engine} profile on port {port}");
        }
      }
    }

    crate::launch_gate::enforce_route_required(profile, upstream_proxy.is_some())
      .map_err(|e| -> BoxError { e.into() })?;

    let (blocklist, gate_result) = tokio::join!(
      Self::resolve_blocklist_file(profile),
      crate::launch_gate::enforce_fingerprint_gate(profile, upstream_proxy.as_ref(), gate),
    );
    gate_result.map_err(|e| -> BoxError { e.into() })?;
    let (blocklist_file, dns_allowlist_mode) = blocklist?;

    // Past the gate: this launch is really happening.
    Self::fire_launch_hook(profile);

    log::info!(
      "Starting local proxy for {engine} profile: {} (upstream: {})",
      profile.name,
      upstream_proxy
        .as_ref()
        .map(|p| format!("{}:{}", p.host, p.port))
        .unwrap_or_else(|| "DIRECT".to_string())
    );

    let placeholder_pid = crate::proxy_manager::next_launch_placeholder_pid();
    let local_proxy = PROXY_MANAGER
      .start_proxy(
        app_handle.clone(),
        upstream_proxy.as_ref(),
        placeholder_pid,
        Some(&profile.id.to_string()),
        profile.proxy_bypass_rules.clone(),
        blocklist_file,
        dns_allowlist_mode,
        Self::records_site_history(profile),
        local_protocol,
      )
      .await
      .map_err(|e| {
        let error_msg =
          crate::wrap_backend_error(e, &format!("Failed to start local proxy for {engine}"));
        log::error!("{}", error_msg);
        error_msg
      })?;

    network.routing_pid = placeholder_pid;
    network.proxy_armed = true;
    network.local_proxy = local_proxy;
    Ok(network)
  }
}
