//! Camoufox browser integration module.
//!
//! Native Rust support for launching Camoufox with realistic fingerprints.
//!
//! - **Fingerprint Generation**: Bayesian network-based fingerprint generation
//!   (a Rust port of BrowserForge)
//! - **WebGL Sampling**: Realistic WebGL configurations from a SQLite database
//! - **Configuration Builder**: Converts fingerprints to Camoufox config format
//!
//! The process itself is spawned by `camoufox_manager`.

pub mod bundled_fonts;
pub mod config;
pub mod data;
pub mod env_vars;
pub mod fingerprint;
pub mod fonts;
pub mod geolocation;
pub mod hook;
pub mod last_window;
pub mod presets;
pub mod profile_app;
pub mod profile_identity;
pub mod search;
pub mod webgl;

// Re-export main types for convenience
pub use config::{
  CamoufoxConfigBuilder, CamoufoxLaunchConfig, ConfigError, GeoIPOption, ProxyConfig,
};
pub use fingerprint::types::{
  Fingerprint, FingerprintOptions, FingerprintWithHeaders, NavigatorFingerprint, ScreenConstraints,
  ScreenFingerprint, VideoCard,
};
pub use fingerprint::{FingerprintError, FingerprintGenerator};
pub use geolocation::{
  fetch_public_ip, get_geolocation, is_geoip_available, is_ipv4, is_ipv6, validate_ip, Geolocation,
  GeolocationError, Locale, LocaleSelector,
};
pub use webgl::{sample_webgl, WebGLData, WebGLError};

/// The directory that holds a Camoufox build's resources (camoufox.cfg,
/// `distribution/`, `defaults/pref/`, `fonts/`): `Contents/Resources` of the
/// macOS app, the executable's own directory elsewhere.
pub(crate) fn resources_dir(executable: &std::path::Path) -> Option<std::path::PathBuf> {
  let dir = executable.parent()?;
  Some(if cfg!(target_os = "macos") {
    dir.parent()?.join("Resources")
  } else {
    dir.to_path_buf()
  })
}

/// Replace a file of a Camoufox build through a file beside it, so a profile
/// of the same build starting at that moment reads the old or the new one.
pub(crate) fn write_atomically(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir)?;
  }
  let staged = staging_path(path);
  std::fs::write(&staged, contents)?;
  std::fs::rename(&staged, path).inspect_err(|_| {
    let _ = std::fs::remove_file(&staged);
  })
}

/// A staging name of its own for every write, so two launches patching the
/// same build file never write through one staged file.
fn staging_path(path: &std::path::Path) -> std::ffi::OsString {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let mut staged = path.as_os_str().to_owned();
  staged.push(format!(".owl-{}-{n}", std::process::id()));
  staged
}

/// Write `contents` only when the file does not already hold exactly that, so
/// re-running a launch hook does not touch the disk. True when it wrote.
pub(crate) fn write_if_changed(path: &std::path::Path, contents: &str) -> std::io::Result<bool> {
  if std::fs::read_to_string(path).is_ok_and(|current| current == contents) {
    return Ok(false);
  }
  write_atomically(path, contents)?;
  Ok(true)
}

/// Unified error type for all Camoufox operations.
#[derive(Debug, thiserror::Error)]
pub enum CamoufoxError {
  #[error("Configuration error: {0}")]
  Config(#[from] ConfigError),

  #[error("Fingerprint error: {0}")]
  Fingerprint(#[from] FingerprintError),

  #[error("WebGL error: {0}")]
  WebGL(#[from] WebGLError),

  #[error("Geolocation error: {0}")]
  Geolocation(#[from] GeolocationError),

  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_fingerprint_generation() {
    let generator = FingerprintGenerator::new().unwrap();
    let options = FingerprintOptions {
      browsers: Some(vec!["firefox".to_string()]),
      operating_system: Some("windows".to_string()),
      ..Default::default()
    };

    let result = generator.get_fingerprint(&options);
    assert!(result.is_ok());

    let fp = result.unwrap();
    assert!(!fp.fingerprint.navigator.user_agent.is_empty());
    assert!(fp.fingerprint.screen.width > 0);
  }

  #[test]
  fn test_config_builder() {
    let config = CamoufoxConfigBuilder::new()
      .operating_system("windows")
      .block_images(false)
      .build();

    assert!(config.is_ok());

    let config = config.unwrap();
    assert!(!config.fingerprint_config.is_empty());
    assert!(config
      .fingerprint_config
      .contains_key("navigator.userAgent"));
  }

  #[test]
  fn test_webgl_sampling() {
    let result = webgl::sample_webgl("win", None, None);
    assert!(result.is_ok());

    let webgl_data = result.unwrap();
    assert!(!webgl_data.vendor.is_empty());
    assert!(!webgl_data.renderer.is_empty());
  }

  #[test]
  fn test_fonts() {
    let fonts = fonts::get_fonts_for_os("win");
    assert!(!fonts.is_empty());
    assert!(fonts.contains(&"Arial".to_string()));
  }

  #[test]
  fn every_atomic_write_stages_through_its_own_file() {
    let path = std::path::Path::new("/build/camoufox.cfg");
    assert_ne!(staging_path(path), staging_path(path));
  }

  #[test]
  fn test_env_vars() {
    let mut config = std::collections::HashMap::new();
    config.insert(
      "navigator.userAgent".to_string(),
      serde_json::json!("Mozilla/5.0"),
    );

    let env_vars = env_vars::config_to_env_vars(&config).unwrap();
    assert!(!env_vars.is_empty());
    assert!(env_vars.contains_key("CAMOU_CONFIG_1"));
  }
}
