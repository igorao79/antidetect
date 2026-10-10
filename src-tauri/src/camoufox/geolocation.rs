//! Camoufox reads geolocation through this path; the implementation lives in
//! the crate-wide `geolocation` and `ip_utils` modules shared with Wayfern.

use std::collections::HashMap;

pub use crate::geolocation::*;
pub use crate::ip_utils::{fetch_public_ip, is_ipv4, is_ipv6, validate_ip};

/// Check if the GeoIP database is available.
pub fn is_geoip_available() -> bool {
  crate::geoip_downloader::GeoIPDownloader::is_geoip_database_available()
}

/// Point a Camoufox fingerprint at the place `ip` is in: timezone, locale,
/// coordinates and, unless WebRTC is blocked, the address WebRTC reports.
/// With a `stable_id` (the profile id) the language is that profile's own for
/// the country; without one it is drawn at random. Leaves the fingerprint
/// alone when the database has nothing for `ip`.
pub fn apply_exit_ip(
  fingerprint: &mut HashMap<String, serde_json::Value>,
  ip: &str,
  block_webrtc: bool,
  stable_id: Option<&str>,
) -> Option<Geolocation> {
  let located = match stable_id {
    Some(id) => get_geolocation_for(ip, id),
    None => get_geolocation(ip),
  };
  let geo = match located {
    Ok(geo) => geo,
    Err(e) => {
      log::warn!("Failed to get geolocation for IP {ip}: {e}");
      return None;
    }
  };
  fingerprint.extend(geo.as_config());
  if !block_webrtc {
    fingerprint.remove("webrtc:ipv4");
    fingerprint.remove("webrtc:ipv6");
    if is_ipv4(ip) {
      fingerprint.insert("webrtc:ipv4".to_string(), serde_json::json!(ip));
    } else if is_ipv6(ip) {
      fingerprint.insert("webrtc:ipv6".to_string(), serde_json::json!(ip));
    }
  }
  log::info!(
    "Applied geolocation from IP {ip}: {} ({})",
    geo.locale.as_string(),
    geo.timezone
  );
  Some(geo)
}

/// Camoufox config keys (`locale:*`, `geolocation:*`, `timezone`) for a
/// resolved location.
pub trait AsCamoufoxConfig {
  fn as_config(&self) -> HashMap<String, serde_json::Value>;
}

impl AsCamoufoxConfig for Locale {
  fn as_config(&self) -> HashMap<String, serde_json::Value> {
    let mut config = HashMap::new();
    if let Some(region) = &self.region {
      config.insert(
        "locale:region".to_string(),
        serde_json::json!(region.to_uppercase()),
      );
    }
    config.insert(
      "locale:language".to_string(),
      serde_json::json!(self.language),
    );
    config
  }
}

impl AsCamoufoxConfig for Geolocation {
  fn as_config(&self) -> HashMap<String, serde_json::Value> {
    let mut config = self.locale.as_config();
    config.insert(
      "geolocation:longitude".to_string(),
      serde_json::json!(self.longitude),
    );
    config.insert(
      "geolocation:latitude".to_string(),
      serde_json::json!(self.latitude),
    );
    config.insert("timezone".to_string(), serde_json::json!(self.timezone));
    config
  }
}
