//! Writes the profiles table's CSV export to the file the person chose (fork).
//!
//! The table is turned into text in the webview, from what the person sees;
//! this only puts it on disk. Excel reads a CSV as the system's legacy code
//! page unless the file starts with a UTF-8 byte order mark, which turns
//! Cyrillic into noise, so the mark is always written.

use std::path::Path;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
/// A few hundred profiles are well under a megabyte; more than this is a bug.
const MAX_EXPORT_BYTES: usize = 32 * 1024 * 1024;

fn with_bom(content: &str) -> Vec<u8> {
  let bytes = content.as_bytes();
  if bytes.starts_with(UTF8_BOM) {
    return bytes.to_vec();
  }
  let mut out = Vec::with_capacity(UTF8_BOM.len() + bytes.len());
  out.extend_from_slice(UTF8_BOM);
  out.extend_from_slice(bytes);
  out
}

fn is_csv_path(path: &Path) -> bool {
  path
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
}

fn write_csv(path: &Path, content: &str) -> Result<(), String> {
  if !is_csv_path(path) {
    return Err(crate::backend_error("PROFILES_EXPORT_PATH_INVALID"));
  }
  if content.len() > MAX_EXPORT_BYTES {
    return Err(crate::backend_error("PROFILES_EXPORT_TOO_LARGE"));
  }
  std::fs::write(path, with_bom(content))
    .map_err(|e| crate::backend_error_with_detail("PROFILES_EXPORT_FAILED", e))
}

/// Saves `content` as a UTF-8 CSV with a byte order mark at `path`, which the
/// save dialog returned.
#[tauri::command]
pub fn write_profiles_csv(path: String, content: String) -> Result<(), String> {
  write_csv(Path::new(&path), &content)?;
  log::info!("Profiles exported bytes={}", content.len());
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_file_starts_with_one_byte_order_mark_and_keeps_cyrillic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.csv");
    write_csv(&path, "name\nПрофиль\n").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(UTF8_BOM));
    assert!(!bytes[3..].starts_with(UTF8_BOM));
    assert_eq!(
      String::from_utf8(bytes[3..].to_vec()).unwrap(),
      "name\nПрофиль\n"
    );

    write_csv(&path, "\u{feff}a\n").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"\xEF\xBB\xBFa\n");
  }

  #[test]
  fn only_csv_paths_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.sh");
    let err = write_csv(&path, "x").unwrap_err();
    assert!(err.contains("PROFILES_EXPORT_PATH_INVALID"));
    assert!(!path.exists());
  }
}
