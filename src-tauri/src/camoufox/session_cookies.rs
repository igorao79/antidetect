//! Session cookies of a Camoufox profile (fork).
//!
//! Firefox writes only cookies with an expiry to `cookies.sqlite`. Session
//! cookies, the ones many sites keep a login in, live in the session file:
//! `sessionstore.jsonlz4` after a clean quit and
//! `sessionstore-backups/recovery.jsonlz4` while the browser runs (and at
//! every launch, where `resume_as_recovery` moves the first to the second).
//! Reading only the database left them out of every export and copy, so a
//! transferred login was gone in the other profile.

use std::path::{Path, PathBuf};

use crate::cookie_manager::UnifiedCookie;

/// `mozLz40\0`, then the decompressed size as a little-endian u32, then one
/// LZ4 block.
const MOZLZ4_MAGIC: &[u8; 8] = b"mozLz40\0";
/// A session file holds tabs, form data and storage; real ones stay well
/// below this, and a corrupt size field must not allocate gigabytes.
const MAX_SESSION_BYTES: usize = 256 * 1024 * 1024;

/// The session cookies of the profile whose Firefox profile directory is
/// `profile_dir`, from the newer of its two session files. No file, an
/// unreadable one or one without cookies is no session cookies.
pub fn read(profile_dir: &Path) -> Vec<UnifiedCookie> {
  let Some(path) = newest_session_file(profile_dir) else {
    return Vec::new();
  };
  match std::fs::read(&path)
    .map_err(|e| e.to_string())
    .and_then(|bytes| decompress_mozlz4(&bytes))
    .and_then(|json| cookies_from_session_json(&json))
  {
    Ok(cookies) => cookies,
    Err(e) => {
      log::warn!("Session cookies unreadable err=\"{e}\"");
      Vec::new()
    }
  }
}

fn newest_session_file(profile_dir: &Path) -> Option<PathBuf> {
  [
    profile_dir.join("sessionstore.jsonlz4"),
    profile_dir
      .join("sessionstore-backups")
      .join("recovery.jsonlz4"),
  ]
  .into_iter()
  .filter_map(|path| {
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    Some((modified, path))
  })
  .max_by_key(|(modified, _)| *modified)
  .map(|(_, path)| path)
}

fn decompress_mozlz4(bytes: &[u8]) -> Result<Vec<u8>, String> {
  let header = MOZLZ4_MAGIC.len() + 4;
  if bytes.len() < header || &bytes[..MOZLZ4_MAGIC.len()] != MOZLZ4_MAGIC {
    return Err("not a mozlz4 file".to_string());
  }
  let size_bytes: [u8; 4] = bytes[MOZLZ4_MAGIC.len()..header]
    .try_into()
    .map_err(|_| "truncated mozlz4 header".to_string())?;
  let size = u32::from_le_bytes(size_bytes) as usize;
  if size > MAX_SESSION_BYTES {
    return Err(format!("session file too large size={size}"));
  }
  lz4_flex::block::decompress(&bytes[header..], size).map_err(|e| e.to_string())
}

fn cookies_from_session_json(json: &[u8]) -> Result<Vec<UnifiedCookie>, String> {
  let session: serde_json::Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
  let Some(entries) = session.get("cookies").and_then(|c| c.as_array()) else {
    return Ok(Vec::new());
  };
  Ok(entries.iter().filter_map(session_cookie).collect())
}

/// One entry of the session file's `cookies`, which Firefox writes with only
/// the fields that differ from their defaults (`SessionCookies.sys.mjs`).
fn session_cookie(entry: &serde_json::Value) -> Option<UnifiedCookie> {
  let host = entry.get("host")?.as_str()?.to_string();
  let text = |key: &str| entry.get(key).and_then(|v| v.as_str()).unwrap_or_default();
  let flag = |key: &str| entry.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
  if host.is_empty() {
    return None;
  }
  let path = match text("path") {
    "" => "/".to_string(),
    path => path.to_string(),
  };
  Some(UnifiedCookie {
    name: text("name").to_string(),
    value: text("value").to_string(),
    domain: host,
    path,
    // 0 is a session cookie; the writers give it a lifetime of its own.
    expires: 0,
    is_secure: flag("secure"),
    is_http_only: flag("httponly"),
    same_site: entry
      .get("sameSite")
      .and_then(|v| v.as_i64())
      .and_then(|v| i32::try_from(v).ok())
      .unwrap_or(0),
    creation_time: 0,
    last_accessed: 0,
  })
}

/// The database's cookies and the session file's, without repeating one the
/// database already holds (same host, name and path).
pub fn merge(mut stored: Vec<UnifiedCookie>, session: Vec<UnifiedCookie>) -> Vec<UnifiedCookie> {
  for cookie in session {
    let known = stored
      .iter()
      .any(|c| c.domain == cookie.domain && c.name == cookie.name && c.path == cookie.path);
    if !known {
      stored.push(cookie);
    }
  }
  stored
}

#[cfg(test)]
mod tests {
  use super::*;

  fn mozlz4(json: &str) -> Vec<u8> {
    let mut out = MOZLZ4_MAGIC.to_vec();
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&lz4_flex::block::compress(json.as_bytes()));
    out
  }

  const SESSION: &str = r#"{"windows":[],"cookies":[
    {"host":"httpbin.org","value":"moved-by-owl-42","name":"owl_session","path":"/","originAttributes":{}},
    {"host":".github.com","value":"abc","name":"_gh_sess","secure":true,"httponly":true,"sameSite":1}
  ]}"#;

  #[test]
  fn session_cookies_come_out_of_a_mozlz4_session_file() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("sessionstore.jsonlz4"), mozlz4(SESSION)).unwrap();

    let cookies = read(tmp.path());
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0].name, "owl_session");
    assert_eq!(cookies[0].value, "moved-by-owl-42");
    assert_eq!(cookies[0].expires, 0, "a session cookie");
    let gh = &cookies[1];
    assert!(gh.is_secure && gh.is_http_only);
    assert_eq!((gh.same_site, gh.path.as_str()), (1, "/"));
  }

  #[test]
  fn the_running_browsers_recovery_file_is_read_when_it_is_newer() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
      tmp.path().join("sessionstore.jsonlz4"),
      mozlz4(r#"{"cookies":[{"host":"old.example","name":"a","value":"1"}]}"#),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::create_dir_all(tmp.path().join("sessionstore-backups")).unwrap();
    std::fs::write(
      tmp.path().join("sessionstore-backups/recovery.jsonlz4"),
      mozlz4(r#"{"cookies":[{"host":"new.example","name":"b","value":"2"}]}"#),
    )
    .unwrap();
    let cookies = read(tmp.path());
    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].domain, "new.example");
  }

  #[test]
  fn no_session_file_or_a_broken_one_is_no_cookies() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(read(tmp.path()).is_empty());
    std::fs::write(tmp.path().join("sessionstore.jsonlz4"), b"garbage").unwrap();
    assert!(read(tmp.path()).is_empty());
    let mut huge = MOZLZ4_MAGIC.to_vec();
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(decompress_mozlz4(&huge).unwrap_err().contains("too large"));
  }

  #[test]
  fn a_cookie_the_database_holds_is_not_repeated() {
    let stored = vec![UnifiedCookie {
      name: "_gh_sess".to_string(),
      value: "db".to_string(),
      domain: ".github.com".to_string(),
      path: "/".to_string(),
      expires: 1,
      is_secure: true,
      is_http_only: true,
      same_site: 1,
      creation_time: 0,
      last_accessed: 0,
    }];
    let session = cookies_from_session_json(SESSION.as_bytes()).unwrap();
    let merged = merge(stored, session);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].value, "db");
    assert_eq!(merged[1].name, "owl_session");
  }
}
