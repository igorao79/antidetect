//! Camoufox reads geolocation through this path; the implementation lives in
//! the crate-wide `geolocation` and `ip_utils` modules shared with Wayfern.

use std::collections::HashMap;

pub use crate::geolocation::*;
pub use crate::ip_utils::{fetch_public_ip, is_ipv4, is_ipv6, validate_ip};

/// Check if the GeoIP database is available.
pub fn is_geoip_available() -> bool {
  crate::geoip_downloader::GeoIPDownloader::is_geoip_database_available()
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
