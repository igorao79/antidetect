//! The CloakBrowser Pro licence key as the app manages it (fork).
//!
//! The key lives where the Pro binary itself looks, `~/.cloakbrowser/license.key`,
//! so a key saved here also serves the vendor's own tools. It is never logged
//! and never handed to the frontend: the interface is shown its last characters.

use crate::api_client::ApiClient;
use serde::Serialize;
use std::path::Path;

const HINT_CHARS: usize = 4;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LicenseStatus {
  /// Where the key in effect comes from: `environment`, `file`, or nothing.
  pub source: Option<&'static str>,
  /// The end of the key, enough to tell two keys apart.
  pub hint: Option<String>,
  /// The licence server's verdict; absent when it was not asked or did not answer.
  pub valid: Option<bool>,
  pub plan: Option<String>,
  pub expires: Option<String>,
}

fn error(code: &str) -> String {
  serde_json::json!({ "code": code }).to_string()
}

fn hint(key: &str) -> String {
  let chars: Vec<char> = key.chars().collect();
  chars[chars.len().saturating_sub(HINT_CHARS)..]
    .iter()
    .collect()
}

fn key_in_effect() -> (Option<&'static str>, Option<String>) {
  let from_environment = std::env::var("CLOAKBROWSER_LICENSE_KEY")
    .ok()
    .is_some_and(|key| !key.trim().is_empty());
  match ApiClient::cloakbrowser_license_key() {
    Some(key) if from_environment => (Some("environment"), Some(key)),
    Some(key) => (Some("file"), Some(key)),
    None => (None, None),
  }
}

/// Writes `key` to `file`, readable by this user only.
fn write_key(file: &Path, key: &str) -> std::io::Result<()> {
  let dir = file
    .parent()
    .ok_or_else(|| std::io::Error::other("the licence file has no folder"))?;
  std::fs::create_dir_all(dir)?;
  let staged = file.with_extension("key.tmp");
  std::fs::write(&staged, format!("{key}\n"))?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600))?;
  }
  std::fs::rename(&staged, file)
}

async fn status(ask_server: bool) -> LicenseStatus {
  let (source, key) = key_in_effect();
  let Some(key) = key else {
    return LicenseStatus::default();
  };
  let mut status = LicenseStatus {
    source,
    hint: Some(hint(&key)),
    ..Default::default()
  };
  if ask_server {
    match ApiClient::instance()
      .validate_cloakbrowser_license(&key)
      .await
    {
      Ok(verdict) => {
        status.valid = Some(verdict.valid);
        status.plan = verdict.plan;
        status.expires = verdict.expires;
      }
      Err(e) => log::warn!("Could not check the CloakBrowser licence: {e}"),
    }
  }
  status
}

/// A key compiled into this build (`ANTIDETECT_BUNDLED_CLOAKBROWSER_KEY` at
/// build time), for handing a build to someone together with the licence.
const BUNDLED_KEY: Option<&str> = option_env!("ANTIDETECT_BUNDLED_CLOAKBROWSER_KEY");

/// Puts `key` in `file` once per install: only where no key is set, and never
/// again after `marker` records it, so a key the user removes stays removed.
fn install_key_once(file: &Path, marker: &Path, key: &str) -> std::io::Result<bool> {
  let Some(key) = crate::api_client::clean_cloakbrowser_key(key) else {
    return Ok(false);
  };
  if marker.exists() || file.exists() {
    return Ok(false);
  }
  write_key(file, &key)?;
  if let Some(dir) = marker.parent() {
    std::fs::create_dir_all(dir)?;
  }
  std::fs::write(marker, b"")?;
  Ok(true)
}

/// Gives this install the build's bundled key when it has none of its own.
pub fn install_bundled_key() {
  let Some(key) = BUNDLED_KEY else {
    return;
  };
  if std::env::var("CLOAKBROWSER_LICENSE_KEY").is_ok_and(|key| !key.trim().is_empty()) {
    return;
  }
  let Some(file) = ApiClient::cloakbrowser_license_file() else {
    return;
  };
  let marker = crate::app_dirs::data_subdir().join("cloakbrowser_bundled_key_installed");
  match install_key_once(&file, &marker, key) {
    Ok(true) => log::info!("Installed the CloakBrowser Pro licence key this build carries"),
    Ok(false) => {}
    Err(e) => log::warn!("Could not install the bundled CloakBrowser licence key: {e}"),
  }
}

#[tauri::command]
pub async fn get_cloakbrowser_license(check: bool) -> Result<LicenseStatus, String> {
  Ok(status(check).await)
}

/// Saves `key` once the licence server has accepted it.
#[tauri::command]
pub async fn set_cloakbrowser_license_key(key: String) -> Result<LicenseStatus, String> {
  let key = key.trim();
  if key.is_empty() {
    return Err(error("CLOAKBROWSER_LICENSE_INVALID"));
  }
  let verdict = ApiClient::instance()
    .validate_cloakbrowser_license(key)
    .await
    .map_err(|e| {
      log::warn!("Could not check the CloakBrowser licence: {e}");
      error("CLOAKBROWSER_LICENSE_UNREACHABLE")
    })?;
  if !verdict.valid {
    return Err(error("CLOAKBROWSER_LICENSE_INVALID"));
  }
  let file =
    ApiClient::cloakbrowser_license_file().ok_or_else(|| error("CLOAKBROWSER_LICENSE_SETUP"))?;
  write_key(&file, key).map_err(|e| {
    log::warn!("Could not save the CloakBrowser licence key: {e}");
    error("CLOAKBROWSER_LICENSE_SETUP")
  })?;
  let (source, key) = key_in_effect();
  Ok(LicenseStatus {
    source,
    hint: key.as_deref().map(hint),
    valid: Some(true),
    plan: verdict.plan,
    expires: verdict.expires,
  })
}

/// Removes the saved key. A key set in the environment stays in effect.
#[tauri::command]
pub async fn remove_cloakbrowser_license_key() -> Result<LicenseStatus, String> {
  if let Some(file) = ApiClient::cloakbrowser_license_file() {
    match std::fs::remove_file(&file) {
      Ok(()) => {}
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
      Err(e) => {
        log::warn!("Could not remove the CloakBrowser licence key: {e}");
        return Err(error("CLOAKBROWSER_LICENSE_SETUP"));
      }
    }
  }
  Ok(status(false).await)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_bundled_key_is_installed_once_and_never_over_a_key_of_the_users() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".cloakbrowser").join("license.key");
    let marker = dir.path().join("data").join("installed");

    assert!(install_key_once(&file, &marker, " bundled ").unwrap());
    assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), "bundled");

    // Removed by the user, it does not come back.
    std::fs::remove_file(&file).unwrap();
    assert!(!install_key_once(&file, &marker, "bundled").unwrap());
    assert!(!file.exists());

    // A key the user already has is left alone.
    let other = tempfile::tempdir().unwrap();
    let own = other.path().join("license.key");
    std::fs::write(&own, "own\n").unwrap();
    assert!(!install_key_once(&own, &other.path().join("installed"), "bundled").unwrap());
    assert_eq!(std::fs::read_to_string(&own).unwrap(), "own\n");
    assert!(!install_key_once(&other.path().join("x"), &other.path().join("m"), "  ").unwrap());
  }

  #[test]
  fn the_hint_is_the_end_of_the_key_and_never_more_than_the_key() {
    assert_eq!(hint("cb_live_0123456789"), "6789");
    assert_eq!(hint("ab"), "ab");
    assert_eq!(hint(""), "");
  }

  #[test]
  fn a_saved_key_is_private_to_the_user_and_replaces_the_one_before() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".cloakbrowser").join("license.key");
    write_key(&file, "first").unwrap();
    write_key(&file, "second").unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), "second");
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = std::fs::metadata(&file).unwrap().permissions().mode();
      assert_eq!(mode & 0o777, 0o600);
    }
    assert!(!file.with_extension("key.tmp").exists());
  }
}
