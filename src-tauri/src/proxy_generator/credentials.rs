//! The generator key pair, sealed with the install's vault key. Device only:
//! it is a member's personal key, never synced.

use super::client::Credentials;
use std::path::PathBuf;

const DIR: &str = "proxy-generator";
const FILE: &str = "credentials";
const MAGIC: &[u8; 6] = b"OWLPG1";
const HINT_EDGE: usize = 4;

fn path() -> PathBuf {
  crate::app_dirs::data_dir().join(DIR).join(FILE)
}

pub fn load() -> Result<Option<Credentials>, String> {
  let Some(text) = crate::vault::open(&path(), MAGIC)? else {
    return Ok(None);
  };
  serde_json::from_str(&text)
    .map(Some)
    .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))
}

pub fn save(credentials: &Credentials) -> Result<(), String> {
  let text = serde_json::to_string(credentials)
    .map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?;
  crate::vault::seal(&path(), MAGIC, &text)
}

pub fn clear() -> Result<(), String> {
  match std::fs::remove_file(path()) {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(crate::backend_error_with_detail("INTERNAL_ERROR", e)),
  }
}

/// The key's edges, enough to tell keys apart (`pgk_…ab12`); never the
/// secret.
pub fn key_hint(key: &str) -> String {
  let chars: Vec<char> = key.chars().collect();
  if chars.len() <= HINT_EDGE * 2 {
    return "…".to_string();
  }
  let head: String = chars[..HINT_EDGE].iter().collect();
  let tail: String = chars[chars.len() - HINT_EDGE..].iter().collect();
  format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn credentials_survive_a_round_trip_and_clear_removes_them() {
    let dir = tempfile::TempDir::new().unwrap();
    let _guard = crate::app_dirs::set_test_data_dir(dir.path().to_path_buf());
    assert!(load().unwrap().is_none());
    let pair = Credentials {
      key: "pgk_abcdef123456".into(),
      secret: "top-secret".into(),
    };
    save(&pair).unwrap();
    assert!(load().unwrap() == Some(pair));
    let sealed = std::fs::read(path()).unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains("top-secret"));
    clear().unwrap();
    assert!(load().unwrap().is_none());
    clear().unwrap();
  }

  #[test]
  fn the_hint_shows_only_the_keys_edges() {
    assert_eq!(key_hint("pgk_0123456789abcd"), "pgk_…abcd");
    assert_eq!(key_hint("short"), "…");
  }
}
