//! CloakBrowser (anti-detect Chromium) process management.
//!
//! CloakBrowser takes its fingerprint as command-line switches: a seed fixes the
//! whole device (GPU, canvas/WebGL/audio noise, fonts, hardware), and optional
//! switches override single values. Nothing is injected over CDP, so the
//! launcher's job is to build the switch list and track the process.
//!
//! The manager keeps no state: a profile's running browser is found by its
//! `--user-data-dir`, which also works after the app restarts.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use utoipa::ToSchema;

/// Seeds the CloakBrowser wrapper itself draws from.
const SEED_MIN: u32 = 10_000;
const SEED_MAX: u32 = 99_999;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CloakBrowserConfig {
  /// Fingerprint seed. The same seed is the same device on every launch.
  #[serde(default)]
  pub seed: Option<u32>,
  /// Operating system to present: `windows`, `macos` or `linux`.
  #[serde(default)]
  pub os: Option<String>,
  #[serde(default)]
  pub gpu_vendor: Option<String>,
  #[serde(default)]
  pub gpu_renderer: Option<String>,
  #[serde(default)]
  pub hardware_concurrency: Option<u32>,
  /// `navigator.deviceMemory`, in GB.
  #[serde(default)]
  pub device_memory: Option<u32>,
  #[serde(default)]
  pub screen_width: Option<u32>,
  #[serde(default)]
  pub screen_height: Option<u32>,
  /// IANA timezone. Unset follows the proxy exit.
  #[serde(default)]
  pub timezone: Option<String>,
  /// BCP-47 locale. Unset follows the proxy exit.
  #[serde(default)]
  pub locale: Option<String>,
  /// `auto` (default: WebRTC reports the proxy exit IP), `real` (untouched)
  /// or `block` (no non-proxied UDP at all).
  #[serde(default)]
  pub webrtc_mode: Option<String>,
  /// Canvas/WebGL/audio/client-rect noise. Unset means on.
  #[serde(default)]
  pub noise: Option<bool>,
  /// Draw a new seed on every launch.
  #[serde(default)]
  pub randomize_fingerprint_on_launch: Option<bool>,
}

pub fn generate_seed() -> u32 {
  use rand::RngExt;
  rand::rng().random_range(SEED_MIN..=SEED_MAX)
}

/// What the route a launch uses looks like from outside, when it is known.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExitLocation {
  pub ip: Option<String>,
  pub timezone: Option<String>,
  pub locale: Option<String>,
}

/// The fingerprint switches for `config`. `exit` fills timezone, locale and
/// the WebRTC address the profile does not pin; `proxied` says whether the
/// launch goes through an upstream at all.
pub fn fingerprint_args(
  config: &CloakBrowserConfig,
  exit: &ExitLocation,
  proxied: bool,
) -> Vec<String> {
  let mut args = Vec::new();
  if let Some(seed) = config.seed {
    args.push(format!("--fingerprint={seed}"));
  }
  if let Some(os) = config.os.as_deref().filter(|os| !os.is_empty()) {
    args.push(format!("--fingerprint-platform={os}"));
  }
  let mut push = |switch: &str, value: Option<String>| {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
      args.push(format!("--{switch}={value}"));
    }
  };
  push("fingerprint-gpu-vendor", config.gpu_vendor.clone());
  push("fingerprint-gpu-renderer", config.gpu_renderer.clone());
  push(
    "fingerprint-hardware-concurrency",
    config.hardware_concurrency.map(|v| v.to_string()),
  );
  push(
    "fingerprint-device-memory",
    config.device_memory.map(|v| v.to_string()),
  );
  push(
    "fingerprint-screen-width",
    config.screen_width.map(|v| v.to_string()),
  );
  push(
    "fingerprint-screen-height",
    config.screen_height.map(|v| v.to_string()),
  );
  push(
    "fingerprint-timezone",
    config.timezone.clone().or_else(|| exit.timezone.clone()),
  );
  // The locale is three switches: the fingerprint value (Intl), the UI
  // language, and the accept list behind navigator.languages and the
  // Accept-Language header. Setting only the first leaves the host's languages
  // visible next to a foreign timezone.
  if let Some(locale) = config
    .locale
    .clone()
    .or_else(|| exit.locale.clone())
    .filter(|l| !l.is_empty())
  {
    let language = locale.split(['-', '_']).next().unwrap_or(&locale);
    let accept = if language == locale {
      locale.clone()
    } else {
      format!("{locale},{language}")
    };
    args.push(format!("--fingerprint-locale={locale}"));
    args.push(format!("--lang={locale}"));
    args.push(format!("--accept-lang={accept}"));
  }
  if config.noise == Some(false) {
    args.push("--fingerprint-noise=false".to_string());
  }

  match config.webrtc_mode.as_deref().unwrap_or("auto") {
    "real" => {}
    "block" => {
      args.push("--webrtc-ip-handling-policy=disable_non_proxied_udp".to_string());
      args.push("--force-webrtc-ip-handling-policy".to_string());
    }
    // A direct connection has nothing to hide: its real egress is what every
    // request already shows.
    _ if proxied => args.push(format!(
      "--fingerprint-webrtc-ip={}",
      exit.ip.as_deref().unwrap_or("auto")
    )),
    _ => {}
  }
  args
}

/// Everything a launch needs besides the fingerprint.
pub struct LaunchRequest<'a> {
  pub executable: &'a Path,
  pub profile_path: &'a str,
  pub profile_id: &'a str,
  pub fingerprint_args: Vec<String>,
  /// The local proxy the browser must use, e.g. `socks5://127.0.0.1:1234`.
  pub proxy_url: Option<&'a str>,
  pub url: Option<&'a str>,
  pub remote_debugging_port: u16,
  pub headless: bool,
  pub ephemeral: bool,
  pub extension_paths: &'a [String],
}

fn launch_args(request: &LaunchRequest) -> Vec<String> {
  let mut args = vec![
    format!("--user-data-dir={}", request.profile_path),
    format!("--remote-debugging-port={}", request.remote_debugging_port),
    "--remote-debugging-address=127.0.0.1".to_string(),
    "--no-first-run".to_string(),
    "--no-default-browser-check".to_string(),
    "--disable-background-mode".to_string(),
    "--disable-component-update".to_string(),
    "--hide-crash-restore-bubble".to_string(),
    // Prefetch* / NoStatePrefetch: speculative loads use a network context that
    // can egress directly, past the per-profile proxy. DnsOverHttps / AsyncDns:
    // name resolution must go through the proxy too.
    "--disable-features=DialMediaRouteProvider,DnsOverHttps,AsyncDns,Prefetch,PrefetchProxy,SpeculationRulesPrefetchFuture,NoStatePrefetch".to_string(),
    "--use-mock-keychain".to_string(),
    "--password-store=basic".to_string(),
  ];
  args.extend(request.fingerprint_args.iter().cloned());

  if let Some(proxy) = request.proxy_url {
    args.push(format!("--proxy-server={proxy}"));
    args.push("--dns-prefetch-disable".to_string());
  }
  if request.headless {
    args.push("--headless=new".to_string());
  }
  if request.ephemeral {
    args.push("--disk-cache-size=1".to_string());
    args.push("--disable-breakpad".to_string());
    args.push("--disable-sync".to_string());
  }
  if !request.extension_paths.is_empty() {
    args.push(format!(
      "--load-extension={}",
      request.extension_paths.join(",")
    ));
  }
  if let Some(url) = request.url {
    args.push(url.to_string());
  }
  args
}

/// A running CloakBrowser main process.
#[derive(Debug, Clone, PartialEq)]
pub struct CloakBrowserProcess {
  pub pid: u32,
  pub cdp_port: Option<u16>,
}

pub async fn launch(request: LaunchRequest<'_>) -> Result<CloakBrowserProcess, String> {
  let args = launch_args(&request);
  log::info!(
    "Launching CloakBrowser on CDP port {} for profile {}",
    request.remote_debugging_port,
    request.profile_id
  );

  let mut command = tokio::process::Command::new(request.executable);
  command
    .args(&args)
    .stdin(Stdio::null())
    .stdout(Stdio::null());
  let stderr_log =
    std::env::temp_dir().join(format!("cloakbrowser-stderr-{}.log", request.profile_id));
  match std::fs::File::create(&stderr_log) {
    Ok(file) => {
      command.stderr(Stdio::from(file));
    }
    Err(_) => {
      command.stderr(Stdio::null());
    }
  }

  let mut child = command
    .spawn()
    .map_err(|e| format!("Failed to spawn CloakBrowser: {e}"))?;
  let pid = child
    .id()
    .ok_or("CloakBrowser did not report a process identifier")?;

  // Reap the child and log how it ended; without this an early crash is only
  // ever seen as "process is gone".
  let profile_id = request.profile_id.to_string();
  tokio::spawn(async move {
    match child.wait().await {
      Ok(status) if status.success() => {
        log::info!("CloakBrowser PID {pid} for {profile_id} exited cleanly")
      }
      Ok(status) => log::warn!("CloakBrowser PID {pid} for {profile_id} exited: {status}"),
      Err(e) => log::warn!("Failed to await CloakBrowser PID {pid}: {e}"),
    }
  });

  Ok(CloakBrowserProcess {
    pid,
    cdp_port: Some(request.remote_debugging_port),
  })
}

/// Whether `cmd` is the main browser process of `profile_path`: it names the
/// profile as its user data dir and is not one of Chromium's `--type=` helpers.
fn main_process_cdp_port(cmd: &[&str], profile_path: &str) -> Option<Option<u16>> {
  let user_data_dir = format!("--user-data-dir={profile_path}");
  if !cmd.contains(&user_data_dir.as_str()) || cmd.iter().any(|arg| arg.starts_with("--type=")) {
    return None;
  }
  Some(
    cmd
      .iter()
      .find_map(|arg| arg.strip_prefix("--remote-debugging-port="))
      .and_then(|port| port.parse().ok()),
  )
}

/// The running browser of `profile_path`, if any.
pub fn find_by_profile(profile_path: &str) -> Option<CloakBrowserProcess> {
  use sysinfo::{ProcessRefreshKind, RefreshKind, System};
  let system = System::new_with_specifics(
    RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
  );
  system.processes().iter().find_map(|(pid, process)| {
    let cmd: Vec<&str> = process.cmd().iter().filter_map(|a| a.to_str()).collect();
    main_process_cdp_port(&cmd, profile_path).map(|cdp_port| CloakBrowserProcess {
      pid: pid.as_u32(),
      cdp_port,
    })
  })
}

/// Ask the browser to shut itself down with `Browser.close`, which lets
/// Chromium flush cookies and session state. A browser that is killed instead
/// loses whatever it had not written yet (cookies are flushed lazily).
///
/// The browser may drop the socket before answering; that is it agreeing, so
/// only failing to deliver the command is an error. The caller watches the
/// process, not the reply.
pub async fn close_gracefully(cdp_port: u16) -> Result<(), String> {
  use futures_util::{SinkExt, StreamExt};

  let version: serde_json::Value = reqwest::Client::builder()
    .no_proxy()
    .timeout(std::time::Duration::from_secs(3))
    .build()
    .map_err(|e| e.to_string())?
    .get(format!("http://127.0.0.1:{cdp_port}/json/version"))
    .send()
    .await
    .map_err(|e| format!("DevTools endpoint did not answer: {e}"))?
    .json()
    .await
    .map_err(|e| e.to_string())?;
  let ws_url = version["webSocketDebuggerUrl"]
    .as_str()
    .ok_or("the browser reported no webSocketDebuggerUrl")?;

  let (mut socket, _) = tokio_tungstenite::connect_async(ws_url)
    .await
    .map_err(|e| format!("could not open the DevTools socket: {e}"))?;
  socket
    .send(tokio_tungstenite::tungstenite::Message::text(
      r#"{"id":1,"method":"Browser.close"}"#,
    ))
    .await
    .map_err(|e| format!("could not send Browser.close: {e}"))?;
  let _ = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next()).await;
  Ok(())
}

/// Ask a running browser to open `url` in a new tab over its DevTools endpoint.
pub async fn open_url(cdp_port: u16, url: &str) -> Result<(), String> {
  let endpoint = format!(
    "http://127.0.0.1:{cdp_port}/json/new?{}",
    urlencoding::encode(url)
  );
  let response = reqwest::Client::builder()
    .no_proxy()
    .timeout(std::time::Duration::from_secs(5))
    .build()
    .map_err(|e| e.to_string())?
    .put(&endpoint)
    .send()
    .await
    .map_err(|e| format!("CloakBrowser DevTools endpoint did not answer: {e}"))?;
  if response.status().is_success() {
    Ok(())
  } else {
    Err(format!(
      "CloakBrowser refused to open the URL: HTTP {}",
      response.status().as_u16()
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn seeds_stay_in_the_wrapper_range() {
    for _ in 0..200 {
      let seed = generate_seed();
      assert!((SEED_MIN..=SEED_MAX).contains(&seed), "{seed}");
    }
  }

  #[test]
  fn a_bare_seed_leaves_everything_else_to_the_browser() {
    let config = CloakBrowserConfig {
      seed: Some(42069),
      os: Some("windows".to_string()),
      ..Default::default()
    };
    assert_eq!(
      fingerprint_args(&config, &ExitLocation::default(), false),
      vec!["--fingerprint=42069", "--fingerprint-platform=windows"]
    );
  }

  #[test]
  fn the_exit_fills_what_the_profile_does_not_pin() {
    let exit = ExitLocation {
      ip: Some("203.0.113.7".to_string()),
      timezone: Some("Europe/Berlin".to_string()),
      locale: Some("de-DE".to_string()),
    };
    let follows = fingerprint_args(&CloakBrowserConfig::default(), &exit, true);
    assert!(follows.contains(&"--fingerprint-timezone=Europe/Berlin".to_string()));
    assert!(follows.contains(&"--fingerprint-locale=de-DE".to_string()));
    assert!(follows.contains(&"--lang=de-DE".to_string()));
    assert!(follows.contains(&"--accept-lang=de-DE,de".to_string()));
    assert!(follows.contains(&"--fingerprint-webrtc-ip=203.0.113.7".to_string()));

    let pinned = CloakBrowserConfig {
      timezone: Some("America/New_York".to_string()),
      locale: Some("en-US".to_string()),
      ..Default::default()
    };
    let args = fingerprint_args(&pinned, &exit, true);
    assert!(args.contains(&"--fingerprint-timezone=America/New_York".to_string()));
    assert!(args.contains(&"--fingerprint-locale=en-US".to_string()));
  }

  #[test]
  fn webrtc_modes_map_to_switches() {
    let unknown_exit = ExitLocation::default();
    let auto = fingerprint_args(&CloakBrowserConfig::default(), &unknown_exit, true);
    assert_eq!(auto, vec!["--fingerprint-webrtc-ip=auto"]);

    let direct = fingerprint_args(&CloakBrowserConfig::default(), &unknown_exit, false);
    assert!(direct.is_empty());

    let real = CloakBrowserConfig {
      webrtc_mode: Some("real".to_string()),
      ..Default::default()
    };
    assert!(fingerprint_args(&real, &unknown_exit, true).is_empty());

    let block = CloakBrowserConfig {
      webrtc_mode: Some("block".to_string()),
      ..Default::default()
    };
    assert_eq!(
      fingerprint_args(&block, &unknown_exit, true),
      vec![
        "--webrtc-ip-handling-policy=disable_non_proxied_udp",
        "--force-webrtc-ip-handling-policy"
      ]
    );
  }

  #[test]
  fn overrides_and_noise_become_switches() {
    let config = CloakBrowserConfig {
      gpu_vendor: Some("Intel Inc.".to_string()),
      gpu_renderer: Some("Intel Iris OpenGL Engine".to_string()),
      hardware_concurrency: Some(8),
      device_memory: Some(16),
      screen_width: Some(1920),
      screen_height: Some(1080),
      noise: Some(false),
      webrtc_mode: Some("real".to_string()),
      ..Default::default()
    };
    assert_eq!(
      fingerprint_args(&config, &ExitLocation::default(), false),
      vec![
        "--fingerprint-gpu-vendor=Intel Inc.",
        "--fingerprint-gpu-renderer=Intel Iris OpenGL Engine",
        "--fingerprint-hardware-concurrency=8",
        "--fingerprint-device-memory=16",
        "--fingerprint-screen-width=1920",
        "--fingerprint-screen-height=1080",
        "--fingerprint-noise=false",
      ]
    );
  }

  #[test]
  fn launch_args_route_through_the_proxy_and_end_with_the_url() {
    let extensions = vec!["/ext/a".to_string(), "/ext/b".to_string()];
    let args = launch_args(&LaunchRequest {
      executable: Path::new("/bin/chromium"),
      profile_path: "/profiles/p1",
      profile_id: "p1",
      fingerprint_args: vec!["--fingerprint=12345".to_string()],
      proxy_url: Some("socks5://127.0.0.1:1080"),
      url: Some("https://example.com"),
      remote_debugging_port: 9222,
      headless: true,
      ephemeral: false,
      extension_paths: &extensions,
    });
    assert_eq!(args[0], "--user-data-dir=/profiles/p1");
    assert!(args.contains(&"--remote-debugging-port=9222".to_string()));
    assert!(args.contains(&"--fingerprint=12345".to_string()));
    assert!(args.contains(&"--proxy-server=socks5://127.0.0.1:1080".to_string()));
    assert!(args.contains(&"--headless=new".to_string()));
    assert!(args.contains(&"--load-extension=/ext/a,/ext/b".to_string()));
    assert_eq!(args.last().map(String::as_str), Some("https://example.com"));
  }

  #[test]
  fn only_the_main_process_of_the_profile_matches() {
    let main = [
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
      "--user-data-dir=/profiles/p1",
      "--remote-debugging-port=9222",
    ];
    assert_eq!(
      main_process_cdp_port(&main, "/profiles/p1"),
      Some(Some(9222))
    );

    let helper = [
      "Chromium Helper",
      "--type=renderer",
      "--user-data-dir=/profiles/p1",
    ];
    assert_eq!(main_process_cdp_port(&helper, "/profiles/p1"), None);

    // A prefix of another profile's path must not match.
    assert_eq!(main_process_cdp_port(&main, "/profiles/p"), None);

    let no_port = ["chromium", "--user-data-dir=/profiles/p1"];
    assert_eq!(main_process_cdp_port(&no_port, "/profiles/p1"), Some(None));
  }
}
