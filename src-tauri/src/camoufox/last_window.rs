//! Closing a profile's last window ends the profile, with its tabs kept.
//!
//! On macOS closing a browser's last window does not quit it: the window and
//! its tabs went, the browser stayed running without windows, Owl still showed
//! the profile as running, and at the next launch the session had nothing to
//! reopen (Firefox keeps a last window for restoring only on other systems).
//! `last_window.js` turns the close button, and closing the last tab, into the
//! quit Owl's own stop asks for, so the session is written with the window
//! open. It runs from the build's camoufox.cfg (Firefox's autoconfig), which
//! may run the browser's own code once `general.config.sandbox_enabled` is
//! off; both files of the build are brought up to date before every launch.

use std::path::Path;

const HOOK: &str = include_str!("last_window.js");
const BEGIN: &str = "// Owl: closing the last window quits the browser (begin)";
const END: &str = "// Owl: closing the last window quits the browser (end)";
const AUTOCONFIG_PREFS: &str =
  "// Owl: lets camoufox.cfg run the browser's own code (camoufox/last_window.rs).\n\
pref(\"general.config.sandbox_enabled\", false);\n";

/// camoufox.cfg with the hook in place of any earlier copy of it.
fn with_hook(cfg: &str) -> String {
  let mut kept = cfg.to_string();
  if let (Some(start), Some(end)) = (kept.find(BEGIN), kept.find(END)) {
    if start < end {
      kept.replace_range(start..end + END.len(), "");
    }
  }
  format!("{}\n\n{BEGIN}\n{HOOK}{END}\n", kept.trim_end())
}

fn update(path: &Path, wanted: &str) -> std::io::Result<bool> {
  if std::fs::read_to_string(path).is_ok_and(|current| current == wanted) {
    return Ok(false);
  }
  super::write_atomically(path, wanted)?;
  Ok(true)
}

/// Make closing the last window of the build at `executable` quit it.
pub fn ensure_quit_with_last_window(executable: &Path) {
  if !cfg!(target_os = "macos") {
    return;
  }
  let Some(resources) = super::resources_dir(executable) else {
    return;
  };
  let cfg_path = resources.join("camoufox.cfg");
  let Ok(cfg) = std::fs::read_to_string(&cfg_path) else {
    return;
  };
  let written = update(
    &resources.join("defaults").join("pref").join("owl.js"),
    AUTOCONFIG_PREFS,
  )
  .and_then(|prefs| Ok(update(&cfg_path, &with_hook(&cfg))? || prefs));
  match written {
    Ok(true) => log::info!(
      "Camoufox in {} now quits with its last window",
      resources.display()
    ),
    Ok(false) => {}
    Err(e) => log::warn!(
      "Could not make Camoufox in {} quit with its last window: {e}",
      resources.display()
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn adds_the_hook_once_and_replaces_an_older_one() {
    let cfg = "// camoufox\ndefaultPref(\"a\", 1);\n";
    let once = with_hook(cfg);
    assert!(once.starts_with(cfg.trim_end()));
    assert_eq!(once.matches(BEGIN).count(), 1);
    assert_eq!(with_hook(&once), once);

    let older = format!("{once}\n// after\n").replace("eForceQuit", "eAttemptQuit");
    let updated = with_hook(&older);
    assert_eq!(updated.matches(BEGIN).count(), 1);
    assert!(updated.contains("eForceQuit"));
    assert!(updated.contains("// after"));
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn writes_the_hook_and_the_autoconfig_pref_into_the_build() {
    let root = tempfile::tempdir().unwrap();
    let resources = root.path().join("Camoufox.app/Contents/Resources");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::write(resources.join("camoufox.cfg"), "// camoufox\n").unwrap();
    let executable = root.path().join("Camoufox.app/Contents/MacOS/camoufox");

    ensure_quit_with_last_window(&executable);
    ensure_quit_with_last_window(&executable);

    let cfg = std::fs::read_to_string(resources.join("camoufox.cfg")).unwrap();
    assert_eq!(cfg.matches(BEGIN).count(), 1);
    let prefs = std::fs::read_to_string(resources.join("defaults/pref/owl.js")).unwrap();
    assert!(prefs.contains("general.config.sandbox_enabled\", false"));
  }
}
