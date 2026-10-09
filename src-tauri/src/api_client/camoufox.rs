//! Camoufox releases, read straight from GitHub (daijro/camoufox).
//!
//! Every Camoufox build is tagged as a beta (`v156.0.1-beta.36`); they are the
//! project's regular releases, so all of them are offered.

use super::ApiClient;
use crate::browser::GithubRelease;
use serde::{Deserialize, Serialize};
use std::fs;

const RELEASES_URL: &str = "https://api.github.com/repos/daijro/camoufox/releases?per_page=30";
const CACHE_FILE: &str = "camoufox_releases.json";

#[derive(Debug, Serialize, Deserialize)]
struct CachedCamoufoxReleases {
  releases: Vec<GithubRelease>,
  timestamp: u64,
}

impl ApiClient {
  /// Release asset suffix for this machine, e.g. `-mac.arm64.zip`.
  fn camoufox_asset_suffix() -> Option<String> {
    let (os, arch) = Self::get_platform_info();
    let (os_name, arch_name) = match (os.as_str(), arch.as_str()) {
      ("macos", "arm64") => ("mac", "arm64"),
      ("macos", "x64") => ("mac", "x86_64"),
      ("linux", "x64") => ("lin", "x86_64"),
      ("linux", "arm64") => ("lin", "arm64"),
      ("windows", "x64") => ("win", "x86_64"),
      _ => return None,
    };
    Some(format!("-{os_name}.{arch_name}.zip"))
  }

  /// Whether `release` ships a build for this machine.
  pub fn camoufox_release_supports_platform(release: &GithubRelease) -> bool {
    let Some(suffix) = Self::camoufox_asset_suffix() else {
      return false;
    };
    release.assets.iter().any(|asset| {
      let name = asset.name.to_lowercase();
      name.starts_with("camoufox-") && name.ends_with(&suffix)
    })
  }

  fn load_cached_camoufox_releases(&self, require_fresh: bool) -> Option<Vec<GithubRelease>> {
    let cache_file = Self::get_cache_dir().ok()?.join(CACHE_FILE);
    let content = fs::read_to_string(cache_file).ok()?;
    let cached: CachedCamoufoxReleases = serde_json::from_str(&content).ok()?;
    if require_fresh && !Self::is_cache_valid(cached.timestamp) {
      return None;
    }
    Some(cached.releases)
  }

  fn save_cached_camoufox_releases(
    &self,
    releases: &[GithubRelease],
  ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cache_file = Self::get_cache_dir()?.join(CACHE_FILE);
    let cached = CachedCamoufoxReleases {
      releases: releases.to_vec(),
      timestamp: Self::get_current_timestamp(),
    };
    fs::write(cache_file, serde_json::to_string(&cached)?)?;
    Ok(())
  }

  /// Camoufox releases that have a build for this machine, newest first.
  ///
  /// `no_caching` skips the 10-minute cache. A failed fetch falls back to any
  /// cached list, so an offline machine can still launch what it has.
  pub async fn fetch_camoufox_releases_with_caching(
    &self,
    no_caching: bool,
  ) -> Result<Vec<GithubRelease>, Box<dyn std::error::Error + Send + Sync>> {
    if !no_caching {
      if let Some(cached) = self.load_cached_camoufox_releases(true) {
        return Ok(cached);
      }
    }

    let mut last_err = String::new();
    for attempt in 1..=3 {
      match self
        .client
        .get(RELEASES_URL)
        .header("User-Agent", "antidetect-browser")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
      {
        Ok(response) if response.status().is_success() => {
          match response.json::<Vec<GithubRelease>>().await {
            Ok(releases) => {
              let mut compatible: Vec<GithubRelease> = releases
                .into_iter()
                .filter(|r| !r.draft && Self::camoufox_release_supports_platform(r))
                .collect();
              // RFC 3339 timestamps sort lexically.
              compatible.sort_by(|a, b| b.published_at.cmp(&a.published_at));
              if let Err(e) = self.save_cached_camoufox_releases(&compatible) {
                log::warn!("Failed to cache Camoufox releases: {e}");
              }
              return Ok(compatible);
            }
            Err(e) => last_err = format!("Failed to parse GitHub response: {e}"),
          }
        }
        Ok(response) => last_err = format!("HTTP {}", response.status().as_u16()),
        Err(e) => {
          log::warn!("Camoufox release fetch attempt {attempt}/3 failed: {e}");
          last_err = crate::system_proxy::explain(&e).unwrap_or_else(|| e.to_string());
        }
      }
      if attempt < 3 {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
      }
    }

    if let Some(cached) = self.load_cached_camoufox_releases(false) {
      log::warn!("Using stale Camoufox release list: {last_err}");
      return Ok(cached);
    }
    Err(format!("Failed to fetch Camoufox releases: {last_err}").into())
  }
}
