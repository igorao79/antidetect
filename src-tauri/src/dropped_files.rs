//! Files dropped onto the main window, read for the webview (fork).
//!
//! macOS hands file drops to Tauri's native handler, so the webview's HTML5
//! drop never sees the file and only the native `tauri://drag-drop` event
//! carries its path. Reading that path from the frontend needed an fs scope
//! over the whole home directory, which let any script in the window read and
//! write every file there. Instead the window event records the paths the
//! person actually dropped, and `read_dropped_text_file` reads only those, for
//! a short while after the drop and up to a size limit.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cookie exports and proxy lists are kilobytes; a bigger file is a mistake
/// that would only freeze the text area it lands in.
const MAX_DROPPED_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// How long a drop stays readable. The frontend reads at once; this only
/// bounds how long a path stays reachable.
const DROP_TTL: Duration = Duration::from_secs(60);
/// The window event and the webview's event leave the native handler at
/// about the same moment, so a read may arrive a little before the record.
const RECORD_WAIT: Duration = Duration::from_millis(750);
const RECORD_POLL: Duration = Duration::from_millis(25);

struct RecordedDrop {
  paths: Vec<PathBuf>,
  at: Instant,
}

static LAST_DROP: Mutex<Option<RecordedDrop>> = Mutex::new(None);

/// Record the paths of a drop onto the main window.
pub fn remember(paths: &[PathBuf]) {
  let paths = paths.iter().map(|path| normalized(path)).collect();
  if let Ok(mut last) = LAST_DROP.lock() {
    *last = Some(RecordedDrop {
      paths,
      at: Instant::now(),
    });
  }
}

fn normalized(path: &Path) -> PathBuf {
  std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn was_dropped(path: &Path, now: Instant) -> bool {
  let wanted = normalized(path);
  LAST_DROP.lock().ok().is_some_and(|last| {
    last
      .as_ref()
      .is_some_and(|drop| now.duration_since(drop.at) <= DROP_TTL && drop.paths.contains(&wanted))
  })
}

async fn wait_until_dropped(path: &Path) -> bool {
  let deadline = Instant::now() + RECORD_WAIT;
  loop {
    if was_dropped(path, Instant::now()) {
      return true;
    }
    if Instant::now() >= deadline {
      return false;
    }
    tokio::time::sleep(RECORD_POLL).await;
  }
}

fn read_text(path: &Path, cap: u64) -> Result<String, String> {
  let metadata = std::fs::metadata(path)
    .map_err(|e| crate::backend_error_with_detail("DROPPED_FILE_UNREADABLE", e))?;
  if !metadata.is_file() {
    return Err(crate::backend_error("DROPPED_FILE_UNREADABLE"));
  }
  if metadata.len() > cap {
    return Err(crate::backend_error("DROPPED_FILE_TOO_LARGE"));
  }
  let bytes = std::fs::read(path)
    .map_err(|e| crate::backend_error_with_detail("DROPPED_FILE_UNREADABLE", e))?;
  String::from_utf8(bytes).map_err(|_| crate::backend_error("DROPPED_FILE_NOT_TEXT"))
}

/// The text of a file the person dropped onto the window a moment ago. Any
/// other path is refused, so the webview cannot read files on its own.
#[tauri::command]
pub async fn read_dropped_text_file(path: String) -> Result<String, String> {
  let path = PathBuf::from(path);
  if !wait_until_dropped(&path).await {
    log::warn!("Dropped file read refused, path was not dropped");
    return Err(crate::backend_error("DROPPED_FILE_NOT_ALLOWED"));
  }
  read_text(&path, MAX_DROPPED_FILE_BYTES)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The drop record is process-wide; tests that set it run one at a time.
  static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

  #[test]
  fn only_a_path_that_was_dropped_can_be_read() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let dropped = tmp.path().join("cookies.txt");
    let other = tmp.path().join("id_rsa");
    std::fs::write(&dropped, "sid=1").unwrap();
    std::fs::write(&other, "secret").unwrap();

    remember(std::slice::from_ref(&dropped));

    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .unwrap();
    let read =
      |path: &Path| runtime.block_on(read_dropped_text_file(path.to_string_lossy().to_string()));
    assert_eq!(read(&dropped).unwrap(), "sid=1");
    let refused = read(&other).unwrap_err();
    assert!(refused.contains("DROPPED_FILE_NOT_ALLOWED"), "{refused}");
  }

  #[test]
  fn a_drop_expires() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let dropped = tmp.path().join("old.txt");
    std::fs::write(&dropped, "x").unwrap();
    remember(std::slice::from_ref(&dropped));
    assert!(was_dropped(&dropped, Instant::now()));
    assert!(!was_dropped(
      &dropped,
      Instant::now() + DROP_TTL + Duration::from_secs(1)
    ));
  }

  #[test]
  fn a_large_or_binary_file_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let big = tmp.path().join("big.txt");
    std::fs::write(&big, "0123456789").unwrap();
    assert!(read_text(&big, 4)
      .unwrap_err()
      .contains("DROPPED_FILE_TOO_LARGE"));
    let binary = tmp.path().join("bin");
    std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
    assert!(read_text(&binary, 1024)
      .unwrap_err()
      .contains("DROPPED_FILE_NOT_TEXT"));
    assert!(read_text(tmp.path(), 1024)
      .unwrap_err()
      .contains("DROPPED_FILE_UNREADABLE"));
  }
}
