//! Each concurrent Camoufox window says which profile it is.
//!
//! A stock Firefox window is indistinguishable from the next one: the title is
//! the page's, the icon is the build's, and ten windows of the same build look
//! alike. Owl writes a small identity document next to every profile
//! (`owl-identity.json`), colors the top of the browser chrome through the
//! profile's `chrome/userChrome.css`, and installs a hook in the shared build's
//! `camoufox.cfg` (`profile_identity.js`) that appends the profile's name to
//! every browser window title and puts a short badge on the macOS Dock tile.
//!
//! The identity file is written to the profile the browser is actually started
//! with (`-profile`), which for an ephemeral or password-protected profile is
//! its own temporary directory. The hook reads that file from `<ProfD>` at
//! runtime, so concurrent browsers of the same build never read or overwrite
//! each other's identity. The hook and the last-window quit hook are separate
//! blocks of `camoufox.cfg` and are installed independently.
//!
//! This is not a custom Dock icon: a badge, a window title suffix and colored
//! chrome are what Firefox offers without forking the engine.

use crate::profile::BrowserProfile;
use serde::{Deserialize, Serialize};
use std::path::Path;

const HOOK: &str = include_str!("profile_identity.js");
const BEGIN: &str = "// Owl: profile identity in the title and the Dock (begin)";
const END: &str = "// Owl: profile identity in the title and the Dock (end)";
const CSS_BEGIN: &str = "/* Owl: profile identity (begin) */";
const CSS_END: &str = "/* Owl: profile identity (end) */";
const IDENTITY_FILE: &str = "owl-identity.json";
const CHROME_DIR: &str = "chrome";
const USER_CHROME_FILE: &str = "userChrome.css";

/// The document the browser hook reads from `<ProfD>/owl-identity.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProfileIdentity {
  /// The whole profile name; the title suffix uses it.
  pub name: String,
  /// `#RRGGBB`, lowercased.
  pub color: String,
  /// A short label for the Dock badge: initials, or the first letters of a
  /// one-word name.
  pub badge: String,
}

/// Accept only a strict six-digit hex color, lowercased. Anything else (a name,
/// spaces, a short or long value) is refused so nothing but a color reaches the
/// generated CSS.
pub(crate) fn sanitize_color(value: &str) -> Option<String> {
  let trimmed = value.trim();
  let bytes = trimmed.as_bytes();
  if bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|b| b.is_ascii_hexdigit()) {
    Some(trimmed.to_ascii_lowercase())
  } else {
    None
  }
}

/// A short, stable label for the profile name: up to three initials of a
/// multi-word name, the first three characters of a one-word one, uppercased
/// and capped at six characters so a macOS Dock badge stays readable.
pub(crate) fn badge_from_name(name: &str) -> String {
  let words: Vec<&str> = name
    .split(|c: char| !c.is_alphanumeric())
    .filter(|word| !word.is_empty())
    .collect();
  let mut badge = String::new();
  if words.len() > 1 {
    for word in words.iter().take(3) {
      if let Some(first) = word.chars().next() {
        badge.extend(first.to_uppercase());
      }
    }
  } else if let Some(word) = words.first() {
    for c in word.chars().take(3) {
      badge.extend(c.to_uppercase());
    }
  }
  if badge.is_empty() {
    // A name of punctuation only still gets a label of its own.
    for c in name.trim().chars().take(3) {
      badge.extend(c.to_uppercase());
    }
  }
  if badge.is_empty() {
    return "?".to_string();
  }
  badge.chars().take(6).collect()
}

pub(crate) fn profile_color(profile: &BrowserProfile) -> String {
  profile
    .window_color
    .as_deref()
    .and_then(sanitize_color)
    .unwrap_or_else(|| crate::wayfern_manager::derive_profile_color(&profile.id))
}

fn identity_for(profile: &BrowserProfile) -> ProfileIdentity {
  ProfileIdentity {
    name: profile.name.clone(),
    color: profile_color(profile),
    badge: badge_from_name(&profile.name),
  }
}

fn owl_css(color: &str) -> String {
  format!(
    ":root {{\n  --owl-profile-color: {color};\n}}\n\
     #navigator-toolbox {{\n  box-shadow: inset 0 3px 0 var(--owl-profile-color) !important;\n}}\n\
     #TabsToolbar {{\n  background-image: linear-gradient(to bottom, color-mix(in srgb, var(--owl-profile-color) 55%, transparent), transparent) !important;\n}}\n"
  )
}

/// `existing` with the Owl color block replaced (or appended), leaving any user
/// CSS outside the markers untouched.
pub(crate) fn with_owl_css(existing: &str, color: &str) -> String {
  super::hook::upsert_block(existing, CSS_BEGIN, CSS_END, &owl_css(color))
}

/// `camoufox.cfg` with the identity hook in place of an earlier copy.
pub(crate) fn with_identity_hook(cfg: &str) -> String {
  super::hook::upsert_block(cfg, BEGIN, END, HOOK)
}

/// Write the identity document and the chrome color block into the profile
/// directory the browser is about to open.
pub fn write_profile_identity(profile: &BrowserProfile, profile_dir: &Path) {
  let identity = identity_for(profile);
  match serde_json::to_string_pretty(&identity) {
    Ok(json) => {
      let json = format!("{json}\n");
      match super::write_if_changed(&profile_dir.join(IDENTITY_FILE), &json) {
        Ok(true) => log::debug!("Wrote Owl profile identity profile={}", profile.id),
        Ok(false) => {}
        Err(e) => log::warn!(
          "Could not write {IDENTITY_FILE} profile={} err=\"{e}\"",
          profile.id
        ),
      }
    }
    Err(e) => log::warn!(
      "Could not serialize the Owl profile identity profile={} err=\"{e}\"",
      profile.id
    ),
  }

  let css_path = profile_dir.join(CHROME_DIR).join(USER_CHROME_FILE);
  let existing = match std::fs::read_to_string(&css_path) {
    Ok(css) => css,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
    Err(e) => {
      log::warn!(
        "Could not read {CHROME_DIR}/{USER_CHROME_FILE} profile={} err=\"{e}\"; leaving it alone",
        profile.id
      );
      return;
    }
  };
  let wanted = with_owl_css(&existing, &identity.color);
  if existing == wanted {
    return;
  }
  if let Err(e) = super::write_atomically(&css_path, &wanted) {
    log::warn!(
      "Could not write {CHROME_DIR}/{USER_CHROME_FILE} profile={} err=\"{e}\"",
      profile.id
    );
  }
}

/// Add the identity hook read by the browser to the shared build's
/// `camoufox.cfg`. macOS-first like the last-window hook: the Dock badge is
/// macOS-only, and enabling the title elsewhere only needs this gate gone (the
/// script's Dock part is already guarded by platform).
pub fn ensure_profile_identity_hook(executable: &Path) {
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
  let written = super::hook::ensure_autoconfig_pref(&resources)
    .and_then(|prefs| Ok(super::write_if_changed(&cfg_path, &with_identity_hook(&cfg))? || prefs));
  match written {
    Ok(true) => log::info!(
      "Camoufox in {} now carries the Owl profile identity",
      resources.display()
    ),
    Ok(false) => {}
    Err(e) => log::warn!(
      "Could not add the Owl profile identity to Camoufox in {}: {e}",
      resources.display()
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn profile(name: &str) -> BrowserProfile {
    BrowserProfile {
      id: uuid::Uuid::parse_str("5a5e2c4a-6f4b-4b0e-9d9c-1a2b3c4d5e6f").unwrap(),
      name: name.to_string(),
      window_color: Some("#A1b2C3".to_string()),
      ..BrowserProfile::default()
    }
  }

  #[test]
  fn only_a_six_digit_hex_color_passes() {
    assert_eq!(sanitize_color("#A1b2C3"), Some("#a1b2c3".to_string()));
    assert_eq!(sanitize_color("  #a1b2c3\t"), Some("#a1b2c3".to_string()));
    for bad in [
      "a1b2c3", "#12345", "#1234567", "#gggggg", "#12345g", "", "#",
    ] {
      assert_eq!(sanitize_color(bad), None, "{bad:?} is not a color");
    }
  }

  #[test]
  fn the_badge_is_short_and_stable() {
    assert_eq!(badge_from_name("Shop"), "SHO");
    assert_eq!(badge_from_name("Client EU"), "CE");
    assert_eq!(badge_from_name("Test Profile 1"), "TP1");
    assert_eq!(badge_from_name("My Profile Name Extra"), "MPN");
    assert_eq!(badge_from_name("профиль"), "ПРО");
    assert_eq!(badge_from_name("***"), "***");
    assert_eq!(badge_from_name(""), "?");
    assert_eq!(badge_from_name("a very long name"), "AVL");
    // Uppercasing must not push a badge past the limit.
    let expanded = badge_from_name("ßßß");
    assert_eq!(expanded.chars().count(), 6);
    let first = badge_from_name("Client EU");
    assert_eq!(badge_from_name("Client EU"), first);
  }

  #[test]
  fn the_identity_json_holds_the_full_name_color_and_badge() {
    let identity = identity_for(&profile("Client EU"));
    assert_eq!(
      identity,
      ProfileIdentity {
        name: "Client EU".to_string(),
        color: "#a1b2c3".to_string(),
        badge: "CE".to_string(),
      }
    );
    let value: serde_json::Value =
      serde_json::from_str(&serde_json::to_string(&identity).unwrap()).unwrap();
    assert_eq!(value["name"], "Client EU");
    assert_eq!(value["color"], "#a1b2c3");
    assert_eq!(value["badge"], "CE");
  }

  #[test]
  fn an_invalid_or_missing_color_falls_back_to_the_id_derived_one() {
    for stored in [None, Some("blue".to_string()), Some("#123".to_string())] {
      let mut p = profile("Client EU");
      p.window_color = stored;
      let color = identity_for(&p).color;
      assert_eq!(color, crate::wayfern_manager::derive_profile_color(&p.id));
      assert!(sanitize_color(&color).is_some());
    }
  }

  #[test]
  fn user_chrome_css_keeps_foreign_rules_and_updates_in_place() {
    let user_css = "#urlbar { color: red; }\n";
    let first = with_owl_css(user_css, "#a1b2c3");
    assert!(first.starts_with(user_css));
    assert!(first.contains("--owl-profile-color: #a1b2c3;"));
    assert_eq!(first.matches(CSS_BEGIN).count(), 1);
    assert_eq!(with_owl_css(&first, "#a1b2c3"), first);

    let with_trailing = format!("{first}\n#footer {{ color: blue; }}\n");
    let updated = with_owl_css(&with_trailing, "#112233");
    assert!(updated.starts_with(user_css));
    assert!(updated.ends_with("#footer { color: blue; }\n"));
    assert_eq!(updated.matches(CSS_BEGIN).count(), 1);
    assert!(updated.contains("#112233"));
    assert!(!updated.contains("#a1b2c3"));
  }

  #[test]
  fn writing_the_identity_leaves_user_chrome_css_alone() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    std::fs::create_dir_all(dir.join(CHROME_DIR)).unwrap();
    std::fs::write(
      dir.join(CHROME_DIR).join(USER_CHROME_FILE),
      "#nav-bar { opacity: 0.9; }\n",
    )
    .unwrap();

    let p = profile("Client EU");
    write_profile_identity(&p, dir);
    write_profile_identity(&p, dir);

    let value: serde_json::Value =
      serde_json::from_str(&std::fs::read_to_string(dir.join(IDENTITY_FILE)).unwrap()).unwrap();
    assert_eq!(value["name"], "Client EU");
    assert_eq!(value["badge"], "CE");
    assert_eq!(value["color"], "#a1b2c3");

    let css = std::fs::read_to_string(dir.join(CHROME_DIR).join(USER_CHROME_FILE)).unwrap();
    assert!(css.starts_with("#nav-bar { opacity: 0.9; }\n"));
    assert_eq!(css.matches(CSS_BEGIN).count(), 1);
    assert_eq!(css.matches("--owl-profile-color: #a1b2c3;").count(), 1);
  }

  #[test]
  fn the_identity_hook_does_not_disturb_the_quit_hook() {
    let quit = crate::camoufox::last_window::with_hook("// camoufox\n");
    let both = with_identity_hook(&quit);
    assert!(both.contains("nsIAppStartup.eForceQuit"));
    assert!(both.contains("owl-identity.json"));
    assert_eq!(
      both
        .matches("Owl: closing the last window quits the browser (begin)")
        .count(),
      1
    );
    assert_eq!(both.matches(BEGIN).count(), 1);
    assert_eq!(with_identity_hook(&both), both);
    assert_eq!(
      crate::camoufox::last_window::with_hook(&both),
      both,
      "the quit hook's own update leaves the identity hook where it is"
    );
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn the_hook_and_the_autoconfig_pref_are_written_once() {
    let root = tempfile::tempdir().unwrap();
    let resources = root.path().join("Camoufox.app/Contents/Resources");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::write(resources.join("camoufox.cfg"), "// camoufox\n").unwrap();
    let executable = root.path().join("Camoufox.app/Contents/MacOS/camoufox");

    ensure_profile_identity_hook(&executable);
    ensure_profile_identity_hook(&executable);

    let cfg = std::fs::read_to_string(resources.join("camoufox.cfg")).unwrap();
    assert_eq!(cfg.matches(BEGIN).count(), 1);
    assert!(cfg.contains("owl-identity.json"));
    let prefs = std::fs::read_to_string(resources.join("defaults/pref/owl.js")).unwrap();
    assert!(prefs.contains("general.config.sandbox_enabled\", false"));
  }
}
