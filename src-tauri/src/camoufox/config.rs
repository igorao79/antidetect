//! Camoufox configuration builder.
//!
//! Converts fingerprints to Camoufox configuration format and builds launch options.

use rand::RngExt;
use serde_yaml;
use std::collections::HashMap;
use std::path::Path;

use crate::camoufox::data;
use crate::camoufox::env_vars;
use crate::camoufox::fingerprint::types::*;
use crate::camoufox::fonts;
use crate::camoufox::geolocation;
use crate::camoufox::presets;
use crate::camoufox::webgl;

/// Browserforge mapping from YAML.
type BrowserforgeMapping = HashMap<String, serde_yaml::Value>;

/// Load the browserforge mapping from embedded YAML.
fn load_browserforge_mapping() -> BrowserforgeMapping {
  serde_yaml::from_str(data::BROWSERFORGE_YML).unwrap_or_default()
}

/// Convert a fingerprint to Camoufox configuration.
pub fn from_browserforge(
  fingerprint: &Fingerprint,
  ff_version: Option<u32>,
) -> HashMap<String, serde_json::Value> {
  let mapping = load_browserforge_mapping();
  let mut config = HashMap::new();

  // Convert fingerprint to a JSON value for easier traversal
  let fp_json = serde_json::to_value(fingerprint).unwrap_or_default();

  // Apply mappings recursively
  cast_to_properties(&mut config, &mapping, &fp_json, ff_version);

  // Handle window.screenX and window.screenY
  handle_screen_xy(&mut config, &fingerprint.screen);

  config
}

/// Recursively cast fingerprint properties to Camoufox config format.
fn cast_to_properties(
  config: &mut HashMap<String, serde_json::Value>,
  mapping: &BrowserforgeMapping,
  fingerprint: &serde_json::Value,
  ff_version: Option<u32>,
) {
  if let serde_json::Value::Object(fp_obj) = fingerprint {
    for (key, mapping_value) in mapping {
      let fp_value = fp_obj.get(key);

      match mapping_value {
        serde_yaml::Value::String(target_key) => {
          if let Some(value) = fp_value {
            let mut final_value = value.clone();

            // Handle negative screen values
            if target_key.starts_with("screen.") {
              if let Some(num) = final_value.as_i64() {
                if num < 0 {
                  final_value = serde_json::json!(0);
                }
              }
            }

            // Replace Firefox version in user agent strings
            if let (Some(version), Some(s)) = (ff_version, final_value.as_str()) {
              let replaced = replace_ff_version(s, version);
              final_value = serde_json::json!(replaced);
            }

            config.insert(target_key.clone(), final_value);
          }
        }
        serde_yaml::Value::Mapping(nested_mapping) => {
          if let Some(nested_fp) = fp_value {
            let nested: BrowserforgeMapping = nested_mapping
              .iter()
              .filter_map(|(k, v)| k.as_str().map(|ks| (ks.to_string(), v.clone())))
              .collect();
            cast_to_properties(config, &nested, nested_fp, ff_version);
          }
        }
        _ => {}
      }
    }
  }
}

/// Replace Firefox version in user agent and related strings.
/// Config keys seeding Camoufox's canvas and audio noise.
const NOISE_SEED_KEYS: [&str; 2] = ["canvas:seed", "audio:seed"];

/// Give a stored fingerprint the noise seeds it lacks, derived from `stable_id`
/// (the profile id), so a profile created before seeds were stored hands the
/// browser the same seeds on every launch.
pub fn ensure_noise_seeds(config: &mut HashMap<String, serde_json::Value>, stable_id: &str) {
  for (stream, key) in NOISE_SEED_KEYS.iter().enumerate() {
    if config.contains_key(*key) {
      continue;
    }
    // FNV-1a over the id, one stream per key so the two seeds are unrelated.
    let mut hash: u32 = 0x811c_9dc5 ^ (stream as u32).wrapping_mul(0x9e37_79b9);
    for byte in stable_id.bytes() {
      hash ^= u32::from(byte);
      hash = hash.wrapping_mul(0x0100_0193);
    }
    config.insert(key.to_string(), serde_json::json!(hash.max(1)));
  }
}

fn replace_ff_version(s: &str, version: u32) -> String {
  // The fingerprint data names the Firefox version it was sampled from in two
  // places (`rv:133.0` and `Firefox/133.0`); both must name the engine that
  // actually runs, or feature detection gives the profile away. regex_lite has
  // no lookbehind, so each token is matched with its prefix.
  static RV: std::sync::LazyLock<regex_lite::Regex> =
    std::sync::LazyLock::new(|| regex_lite::Regex::new(r"rv:1[0-9]{2}\.0").unwrap());
  static FIREFOX: std::sync::LazyLock<regex_lite::Regex> =
    std::sync::LazyLock::new(|| regex_lite::Regex::new(r"Firefox/1[0-9]{2}\.0").unwrap());
  let s = RV.replace_all(s, format!("rv:{version}.0").as_str());
  FIREFOX
    .replace_all(&s, format!("Firefox/{version}.0").as_str())
    .into_owned()
}

/// Handle window.screenX and window.screenY generation.
fn handle_screen_xy(config: &mut HashMap<String, serde_json::Value>, screen: &ScreenFingerprint) {
  if config.contains_key("window.screenY") {
    return;
  }

  let screen_x = screen.screen_x;
  if screen_x == 0 {
    config.insert("window.screenX".to_string(), serde_json::json!(0));
    config.insert("window.screenY".to_string(), serde_json::json!(0));
    return;
  }

  if (-50..=50).contains(&screen_x) {
    config.insert("window.screenY".to_string(), serde_json::json!(screen_x));
    return;
  }

  let screen_y = screen.avail_height as i32 - screen.outer_height as i32;
  let mut rng = rand::rng();

  let y = if screen_y == 0 {
    0
  } else if screen_y > 0 {
    rng.random_range(0..=screen_y)
  } else {
    rng.random_range(screen_y..=0)
  };

  config.insert("window.screenY".to_string(), serde_json::json!(y));
}

/// GeoIP option - can be an IP address string or auto-detect.
#[derive(Debug, Clone)]
pub enum GeoIPOption {
  /// Auto-detect IP (fetch public IP, optionally through proxy)
  Auto,
  /// Use a specific IP address
  IP(String),
}

/// Configuration builder for Camoufox launch.
#[derive(Debug, Clone)]
pub struct CamoufoxConfigBuilder {
  fingerprint: Option<Fingerprint>,
  operating_system: Option<String>,
  screen_constraints: Option<ScreenConstraints>,
  block_images: bool,
  block_webrtc: bool,
  block_webgl: bool,
  custom_fonts: Option<Vec<String>>,
  custom_fonts_only: bool,
  firefox_prefs: HashMap<String, serde_json::Value>,
  proxy: Option<ProxyConfig>,
  headless: bool,
  ff_version: Option<u32>,
  extra_config: HashMap<String, serde_json::Value>,
  geoip: Option<GeoIPOption>,
}

/// Proxy configuration.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
  pub server: String,
  pub username: Option<String>,
  pub password: Option<String>,
  pub bypass: Option<String>,
}

impl ProxyConfig {
  /// Parse a proxy URL string into ProxyConfig.
  /// Supports formats like:
  /// - "http://host:port"
  /// - "http://user:pass@host:port"
  /// - "socks5://user:pass@host:port"
  pub fn from_url(url: &str) -> Result<Self, ConfigError> {
    let parsed = url::Url::parse(url).map_err(|e| ConfigError::InvalidProxy(e.to_string()))?;

    let host = parsed
      .host_str()
      .ok_or_else(|| ConfigError::InvalidProxy("Missing host".to_string()))?;

    let port = parsed.port().unwrap_or(8080);
    let scheme = parsed.scheme();

    let server = format!("{scheme}://{host}:{port}");

    let username = if !parsed.username().is_empty() {
      Some(parsed.username().to_string())
    } else {
      None
    };

    let password = parsed.password().map(String::from);

    Ok(Self {
      server,
      username,
      password,
      bypass: None,
    })
  }
}

impl Default for CamoufoxConfigBuilder {
  fn default() -> Self {
    Self::new()
  }
}

impl CamoufoxConfigBuilder {
  pub fn new() -> Self {
    Self {
      fingerprint: None,
      operating_system: None,
      screen_constraints: None,
      block_images: false,
      block_webrtc: false,
      block_webgl: false,
      custom_fonts: None,
      custom_fonts_only: false,
      firefox_prefs: HashMap::new(),
      proxy: None,
      headless: false,
      ff_version: None,
      extra_config: HashMap::new(),
      geoip: None,
    }
  }

  pub fn fingerprint(mut self, fp: Fingerprint) -> Self {
    self.fingerprint = Some(fp);
    self
  }

  pub fn operating_system(mut self, os: &str) -> Self {
    self.operating_system = Some(os.to_string());
    self
  }

  pub fn screen_constraints(mut self, constraints: ScreenConstraints) -> Self {
    self.screen_constraints = Some(constraints);
    self
  }

  pub fn block_images(mut self, block: bool) -> Self {
    self.block_images = block;
    self
  }

  pub fn block_webrtc(mut self, block: bool) -> Self {
    self.block_webrtc = block;
    self
  }

  pub fn block_webgl(mut self, block: bool) -> Self {
    self.block_webgl = block;
    self
  }

  pub fn custom_fonts(mut self, fonts: Vec<String>) -> Self {
    self.custom_fonts = Some(fonts);
    self
  }

  pub fn custom_fonts_only(mut self, only: bool) -> Self {
    self.custom_fonts_only = only;
    self
  }

  pub fn firefox_pref<V: Into<serde_json::Value>>(mut self, key: &str, value: V) -> Self {
    self.firefox_prefs.insert(key.to_string(), value.into());
    self
  }

  pub fn proxy(mut self, proxy: ProxyConfig) -> Self {
    self.proxy = Some(proxy);
    self
  }

  pub fn headless(mut self, headless: bool) -> Self {
    self.headless = headless;
    self
  }

  pub fn ff_version(mut self, version: u32) -> Self {
    self.ff_version = Some(version);
    self
  }

  pub fn extra_config<V: Into<serde_json::Value>>(mut self, key: &str, value: V) -> Self {
    self.extra_config.insert(key.to_string(), value.into());
    self
  }

  /// Set GeoIP option for geolocation-based fingerprinting.
  /// Use `GeoIPOption::Auto` to auto-detect public IP (optionally through proxy).
  /// Use `GeoIPOption::IP(ip_string)` to use a specific IP address.
  pub fn geoip(mut self, option: GeoIPOption) -> Self {
    self.geoip = Some(option);
    self
  }

  /// Build the complete Camoufox launch configuration.
  ///
  /// Prefers a real-fingerprint preset (matched against the Camoufox build's
  /// Firefox version via `presets::preset_line_for`) when no explicit
  /// fingerprint was passed. Falls back to the Bayesian network-based
  /// synthesizer when presets are unavailable, so callers without a known
  /// Firefox version (or with no preset for the requested OS) still get a
  /// valid config — matching pre-v150 behaviour byte-for-byte.
  pub fn build(self) -> Result<CamoufoxLaunchConfig, ConfigError> {
    let mut rng = rand::rng();
    let ff_version = self.ff_version;

    // 1) The caller supplied a fingerprint outright — honour it and skip
    //    presets entirely. This is the path tests and advanced consumers
    //    use to inject deterministic fixtures.
    // 2) Otherwise, try a bundled preset for the requested OS / FF line.
    // 3) Fall back to the Bayesian generator. This is also the path that
    //    runs for users whose Camoufox binary has no readable `version.json`
    //    (`ff_version == None`), or whose OS has no presets bundled.
    let (mut config, target_os) = if let Some(fp) = self.fingerprint {
      let target_os = env_vars::determine_ua_os(&fp.navigator.user_agent);
      // `from_browserforge` already runs `handle_screen_xy` internally.
      let config = from_browserforge(&fp, ff_version);
      (config, target_os)
    } else if let Some(preset) =
      presets::get_random_preset(self.operating_system.as_deref(), ff_version)
    {
      let mut config = presets::from_preset(&preset, ff_version);
      let target_os = config
        .get("navigator.userAgent")
        .and_then(|v| v.as_str())
        .map(env_vars::determine_ua_os)
        .or_else(|| {
          // Last-resort heuristic from the platform string — keeps target_os
          // sensible even if a preset somehow omits the user agent.
          config
            .get("navigator.platform")
            .and_then(|v| v.as_str())
            .map(|p| match p {
              "Win32" => "windows",
              "MacIntel" => "macos",
              _ => "linux",
            })
        })
        .unwrap_or("macos");
      // Presets don't carry multi-monitor offsets, so default screenX/Y to
      // (0, 0) — matches what real single-display users send.
      config
        .entry("window.screenX".to_string())
        .or_insert(serde_json::json!(0));
      config
        .entry("window.screenY".to_string())
        .or_insert(serde_json::json!(0));
      (config, target_os)
    } else {
      let generator = crate::camoufox::fingerprint::FingerprintGenerator::new()?;
      let options = FingerprintOptions {
        operating_system: self.operating_system.clone(),
        browsers: Some(vec!["firefox".to_string()]),
        devices: Some(vec!["desktop".to_string()]),
        screen: self.screen_constraints,
        ..Default::default()
      };
      let fingerprint = generator.get_fingerprint(&options)?.fingerprint;
      let target_os = env_vars::determine_ua_os(&fingerprint.navigator.user_agent);
      let config = from_browserforge(&fingerprint, ff_version);
      (config, target_os)
    };

    // Note: we used to spoof `window.history.length` to a random value in
    // [1, 5] here. Newer Camoufox builds clamp the docShell session history
    // to this value, which disables the toolbar back/forward buttons when
    // the spoof rolls a small number. The fingerprint value drifts on every
    // user navigation anyway, so a constant spoof is detectable and not
    // worth the broken navigation UX.

    // Add fonts
    if !self.custom_fonts_only {
      let system_fonts = fonts::get_fonts_for_os(target_os);
      let fonts = if let Some(custom) = &self.custom_fonts {
        let mut all_fonts = system_fonts;
        for font in custom {
          if !all_fonts.contains(font) {
            all_fonts.push(font.clone());
          }
        }
        all_fonts
      } else {
        system_fonts
      };
      config.insert("fonts".to_string(), serde_json::json!(fonts));
    } else if let Some(custom) = &self.custom_fonts {
      config.insert("fonts".to_string(), serde_json::json!(custom));
    }

    // Add font spacing seed
    config.insert(
      "fonts:spacing_seed".to_string(),
      serde_json::json!(rng.random_range(0..1_073_741_824u32)),
    );

    // Build Firefox preferences
    let mut firefox_prefs = self.firefox_prefs;

    if self.block_images {
      firefox_prefs.insert(
        "permissions.default.image".to_string(),
        serde_json::json!(2),
      );
    }

    if self.block_webrtc {
      firefox_prefs.insert(
        "media.peerconnection.enabled".to_string(),
        serde_json::json!(false),
      );
    }

    if self.block_webgl {
      firefox_prefs.insert("webgl.disabled".to_string(), serde_json::json!(true));
    } else {
      // Sample and add WebGL configuration
      match webgl::sample_webgl(target_os, None, None) {
        Ok(webgl_data) => {
          for (key, value) in webgl_data.config {
            config.insert(key, value);
          }
          firefox_prefs.insert("webgl.force-enabled".to_string(), serde_json::json!(true));
        }
        Err(e) => {
          log::warn!("Failed to sample WebGL config: {}", e);
        }
      }
    }

    // Canvas anti-fingerprinting
    config.insert(
      "canvas:aaOffset".to_string(),
      serde_json::json!(rng.random_range(-50..=50)),
    );
    config.insert("canvas:aaCapOffset".to_string(), serde_json::json!(true));
    // Camoufox draws these per launch when they are absent. Fixing them here
    // makes the seeded noise part of the stored device. Note that 2D canvas
    // output still changes per session: that is stock Firefox's baseline
    // fingerprinting protection (per-session randomization), which Camoufox
    // 156 does not override and which every regular Firefox shows too.
    for key in NOISE_SEED_KEYS {
      config.insert(
        key.to_string(),
        serde_json::json!(rng.random_range(1..=u32::MAX)),
      );
    }

    // Add extra config (user-provided)
    for (key, value) in self.extra_config {
      config.insert(key, value);
    }

    // Hardcoded Camoufox settings (cannot be overridden)
    // Disable theming to prevent fingerprinting via browser theme
    config.insert("disableTheming".to_string(), serde_json::json!(true));
    // Hide cursor in headless mode
    config.insert("showcursor".to_string(), serde_json::json!(false));

    Ok(CamoufoxLaunchConfig {
      fingerprint_config: config,
      firefox_prefs,
      proxy: self.proxy,
      headless: self.headless,
      target_os: target_os.to_string(),
    })
  }

  /// Build the complete Camoufox launch configuration with async geolocation support.
  /// This method should be used when geoip option is set to Auto.
  pub async fn build_async(self) -> Result<CamoufoxLaunchConfig, ConfigError> {
    // Get full proxy URL (with credentials) for IP detection
    let proxy_url = self.proxy.as_ref().map(|p| {
      if let (Some(user), Some(pass)) = (&p.username, &p.password) {
        // Reconstruct URL with credentials: scheme://user:pass@host:port
        if let Ok(mut parsed) = url::Url::parse(&p.server) {
          let _ = parsed.set_username(user);
          let _ = parsed.set_password(Some(pass));
          parsed.to_string()
        } else {
          p.server.clone()
        }
      } else if let Some(user) = &p.username {
        if let Ok(mut parsed) = url::Url::parse(&p.server) {
          let _ = parsed.set_username(user);
          parsed.to_string()
        } else {
          p.server.clone()
        }
      } else {
        p.server.clone()
      }
    });
    let geoip_option = self.geoip.clone();
    let block_webrtc = self.block_webrtc;

    // Build base config first
    let mut launch_config = self.build()?;

    // Handle geolocation if geoip option is set
    if let Some(geoip) = geoip_option {
      let ip = match geoip {
        GeoIPOption::Auto => {
          // Fetch public IP, optionally through proxy
          geolocation::fetch_public_ip(proxy_url.as_deref())
            .await
            .map_err(geolocation::GeolocationError::from)?
        }
        GeoIPOption::IP(ip_str) => {
          if !geolocation::validate_ip(&ip_str) {
            return Err(ConfigError::Geolocation(
              geolocation::GeolocationError::InvalidIP(ip_str),
            ));
          }
          ip_str
        }
      };

      // A location the database cannot place is left out rather than failing
      // the build.
      geolocation::apply_exit_ip(&mut launch_config.fingerprint_config, &ip, block_webrtc);
    }

    Ok(launch_config)
  }
}

/// Complete Camoufox launch configuration.
#[derive(Debug, Clone)]
pub struct CamoufoxLaunchConfig {
  pub fingerprint_config: HashMap<String, serde_json::Value>,
  pub firefox_prefs: HashMap<String, serde_json::Value>,
  pub proxy: Option<ProxyConfig>,
  pub headless: bool,
  pub target_os: String,
}

impl CamoufoxLaunchConfig {
  /// Get environment variables for launching Camoufox.
  pub fn get_env_vars(&self) -> Result<HashMap<String, String>, serde_json::Error> {
    env_vars::config_to_env_vars(&self.fingerprint_config)
  }

  /// Get the config as JSON string.
  pub fn config_json(&self) -> Result<String, serde_json::Error> {
    serde_json::to_string(&self.fingerprint_config)
  }
}

/// Error type for configuration operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
  #[error("Fingerprint generation error: {0}")]
  Fingerprint(#[from] crate::camoufox::fingerprint::FingerprintError),

  #[error("JSON error: {0}")]
  Json(#[from] serde_json::Error),

  #[error("WebGL error: {0}")]
  WebGL(#[from] webgl::WebGLError),

  #[error("Invalid proxy configuration: {0}")]
  InvalidProxy(String),

  #[error("Geolocation error: {0}")]
  Geolocation(#[from] crate::camoufox::geolocation::GeolocationError),
}

/// Get Firefox version from executable path.
pub fn get_firefox_version(executable_path: &Path) -> Option<u32> {
  // The bundle states its own version (`Version=156.0.1-beta.36`): macOS keeps
  // application.ini in Contents/Resources, Linux and Windows next to the binary.
  let dir = executable_path.parent()?;
  for ini in [
    dir.join("application.ini"),
    dir.join("..").join("Resources").join("application.ini"),
  ] {
    if let Ok(content) = std::fs::read_to_string(&ini) {
      if let Some(major) = content
        .lines()
        .find_map(|line| line.strip_prefix("Version="))
        .and_then(parse_major_version)
      {
        return Some(major);
      }
    }
  }

  // Fallback: the version.json the downloader writes at the version root,
  // a few levels above the binary inside an .app bundle.
  executable_path.ancestors().skip(1).take(5).find_map(|dir| {
    let content = std::fs::read_to_string(dir.join("version.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    parse_major_version(json.get("version")?.as_str()?)
  })
}

/// `156` from `156.0.1-beta.36` or `v156.0.1`.
fn parse_major_version(version: &str) -> Option<u32> {
  version
    .trim()
    .trim_start_matches(['v', 'V'])
    .split('.')
    .next()?
    .parse()
    .ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generated_fingerprints_carry_noise_seeds() {
    let config = CamoufoxConfigBuilder::new()
      .operating_system("windows")
      .build()
      .unwrap();
    for key in NOISE_SEED_KEYS {
      let seed = config.fingerprint_config.get(key).and_then(|v| v.as_u64());
      assert!(seed.is_some_and(|s| s > 0), "{key}: {seed:?}");
    }
  }

  #[test]
  fn missing_noise_seeds_are_derived_from_the_profile_id() {
    let mut first = HashMap::new();
    ensure_noise_seeds(&mut first, "profile-a");
    let mut again = HashMap::new();
    ensure_noise_seeds(&mut again, "profile-a");
    let mut other = HashMap::new();
    ensure_noise_seeds(&mut other, "profile-b");

    assert_eq!(first, again, "the same profile must get the same seeds");
    assert_ne!(first["canvas:seed"], other["canvas:seed"]);
    assert_ne!(first["canvas:seed"], first["audio:seed"]);

    // A stored seed is the profile's own and is never replaced.
    let mut stored = HashMap::from([("canvas:seed".to_string(), serde_json::json!(7))]);
    ensure_noise_seeds(&mut stored, "profile-a");
    assert_eq!(stored["canvas:seed"], serde_json::json!(7));
    assert_eq!(stored["audio:seed"], first["audio:seed"]);
  }

  #[test]
  fn firefox_version_is_rewritten_in_user_agent() {
    assert_eq!(
      replace_ff_version(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
        156
      ),
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:156.0) Gecko/20100101 Firefox/156.0"
    );
  }

  #[test]
  fn firefox_major_version_parses_camoufox_tags() {
    assert_eq!(parse_major_version("156.0.1-beta.36"), Some(156));
    assert_eq!(parse_major_version("v156.0.1"), Some(156));
    assert_eq!(parse_major_version("beta"), None);
  }

  #[test]
  fn firefox_version_is_read_from_the_app_bundle() {
    let root = std::env::temp_dir().join(format!("antidetect-ffver-{}", uuid::Uuid::new_v4()));
    let macos = root.join("Camoufox.app/Contents/MacOS");
    let resources = root.join("Camoufox.app/Contents/Resources");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::write(
      resources.join("application.ini"),
      "[App]\nName=Camoufox\nVersion=156.0.1-beta.36\n",
    )
    .unwrap();
    assert_eq!(get_firefox_version(&macos.join("camoufox")), Some(156));

    std::fs::remove_file(resources.join("application.ini")).unwrap();
    std::fs::write(root.join("version.json"), r#"{"version":"v150.0.2"}"#).unwrap();
    assert_eq!(get_firefox_version(&macos.join("camoufox")), Some(150));
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn test_config_builder() {
    let config = CamoufoxConfigBuilder::new()
      .operating_system("windows")
      .block_images(true)
      .build();

    assert!(config.is_ok());
    let config = config.unwrap();
    assert!(config
      .firefox_prefs
      .contains_key("permissions.default.image"));
  }

  #[test]
  fn test_replace_ff_version() {
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:135.0) Gecko/20100101 Firefox/135.0";
    let replaced = replace_ff_version(ua, 140);
    assert!(replaced.contains("140.0"));
  }

  #[test]
  fn test_from_browserforge() {
    let fingerprint = Fingerprint {
      screen: ScreenFingerprint {
        width: 1920,
        height: 1080,
        avail_width: 1920,
        avail_height: 1040,
        color_depth: 24,
        pixel_depth: 24,
        inner_width: 1903,
        inner_height: 969,
        outer_width: 1920,
        outer_height: 1040,
        ..Default::default()
      },
      navigator: NavigatorFingerprint {
        user_agent: "Mozilla/5.0 Firefox/135.0".to_string(),
        platform: "Win32".to_string(),
        language: "en-US".to_string(),
        languages: vec!["en-US".to_string()],
        hardware_concurrency: 8,
        ..Default::default()
      },
      ..Default::default()
    };

    let config = from_browserforge(&fingerprint, Some(140));

    assert!(config.contains_key("navigator.userAgent"));
    assert!(config.contains_key("screen.width"));
  }
}
