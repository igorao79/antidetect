//! The language a profile's browser shows its own interface in (fork).
//!
//! Neither engine keeps the interface language apart from what a page can
//! read, so by default the interface follows the profile's own language and
//! the two cannot disagree. Measured (`s14_interface_language.py`):
//!
//! - Camoufox takes everything from `intl.locale.requested`: menus, `Intl`,
//!   date and number formats, form messages. Left unset it stays `en-US`
//!   under any `navigator.language`. Its translations ship inside the build.
//! - Chromium on macOS ignores `--lang` and takes the interface language from
//!   the system's `AppleLanguages`. The free CloakBrowser build derives `Intl`
//!   and form messages from it as well; a Pro build pins `Intl` with
//!   `--fingerprint-locale` and leaks only the form messages.
//!
//! A profile may ask for another interface language (`interface_language`),
//! which a page can then notice through exactly those surfaces.

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// How long a starting Chromium is given to read its language before the
/// override is taken back.
const SETTLE: Duration = Duration::from_millis(1500);

const LANGUAGES_KEY: &str = "AppleLanguages";

/// Launches that override the language go one at a time: the override lives
/// in the engine's preferences, which every instance of the build shares.
static OVERRIDE_LOCK: LazyLock<Arc<tokio::sync::Mutex<()>>> =
  LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));

/// A language tag as it may be handed to a browser: letters, digits and
/// hyphens, nothing else.
pub fn clean_tag(raw: &str) -> Option<String> {
  let tag = raw.trim().replace('_', "-");
  let valid = !tag.is_empty()
    && tag.len() <= 35
    && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    && tag.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
  valid.then_some(tag)
}

/// The language a Camoufox fingerprint gives the profile, e.g. `de-DE`.
pub fn camoufox_profile_locale(fingerprint: &HashMap<String, Value>) -> Option<String> {
  let text = |key: &str| {
    fingerprint
      .get(key)
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|value| !value.is_empty())
  };
  let language = text("locale:language")?;
  let locale = match text("locale:region") {
    Some(region) => format!("{language}-{}", region.to_uppercase()),
    None => language.to_string(),
  };
  clean_tag(&locale)
}

/// The startup preference that sets Camoufox's language, as the environment
/// variable its build reads prefs from: the profile's own language unless the
/// profile asks for another interface language.
pub fn camoufox_env(
  fingerprint: &HashMap<String, Value>,
  interface_language: Option<&str>,
) -> Option<(String, String)> {
  let language = interface_language
    .and_then(clean_tag)
    .or_else(|| camoufox_profile_locale(fingerprint))?;
  Some((
    "CAMOU_PREFS_1".to_string(),
    serde_json::json!({ "intl.locale.requested": language }).to_string(),
  ))
}

/// The `.app` bundle an executable inside it belongs to.
fn bundle_of(executable: &Path) -> Option<PathBuf> {
  executable
    .ancestors()
    .find(|dir| dir.extension().is_some_and(|ext| ext == "app"))
    .map(Path::to_path_buf)
}

async fn defaults(args: &[&str]) -> Option<String> {
  let output = tokio::process::Command::new("/usr/bin/defaults")
    .args(args)
    .output()
    .await
    .ok()?;
  output
    .status
    .success()
    .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

/// The entries of a `defaults read` array, as it prints them.
fn array_entries(printed: &str) -> Vec<String> {
  printed
    .lines()
    .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
    .filter(|entry| !entry.is_empty() && *entry != "(" && *entry != ")")
    .map(str::to_string)
    .collect()
}

async fn write_languages(domain: &str, languages: &[String]) {
  let mut args = vec!["write", domain, LANGUAGES_KEY, "-array"];
  args.extend(languages.iter().map(String::as_str));
  if defaults(&args).await.is_none() {
    log::warn!("Could not set the interface language of {domain}");
  }
}

/// Runs `spawn` with the Chromium build at `executable` set to show its
/// interface in `language`. The override is in place only while the browser
/// starts; what the preference held before is put back afterwards.
pub async fn spawn_chromium_in<T>(
  executable: &Path,
  language: Option<&str>,
  spawn: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
  let Some(language) = language
    .and_then(clean_tag)
    .filter(|_| cfg!(target_os = "macos"))
  else {
    return spawn();
  };
  let info = bundle_of(executable).map(|bundle| bundle.join("Contents").join("Info"));
  let domain = match info {
    Some(info) => defaults(&["read", &info.to_string_lossy(), "CFBundleIdentifier"]).await,
    None => None,
  };
  let Some(domain) = domain
    .map(|id| id.trim().to_string())
    .filter(|id| !id.is_empty())
  else {
    log::warn!("The browser's bundle identifier is unknown; its interface language is left alone");
    return spawn();
  };

  let guard = OVERRIDE_LOCK.clone().lock_owned().await;
  let before = defaults(&["read", &domain, LANGUAGES_KEY])
    .await
    .map(|printed| array_entries(&printed))
    .filter(|entries| !entries.is_empty());
  write_languages(&domain, &[language]).await;
  let spawned = spawn();
  tokio::spawn(async move {
    tokio::time::sleep(SETTLE).await;
    match before {
      Some(entries) => write_languages(&domain, &entries).await,
      None => {
        defaults(&["delete", &domain, LANGUAGES_KEY]).await;
      }
    }
    drop(guard);
  });
  spawned
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn fingerprint(pairs: &[(&str, &str)]) -> HashMap<String, Value> {
    pairs
      .iter()
      .map(|(key, value)| (key.to_string(), json!(value)))
      .collect()
  }

  #[test]
  fn a_tag_is_letters_digits_and_hyphens() {
    assert_eq!(clean_tag(" de_DE ").as_deref(), Some("de-DE"));
    assert_eq!(clean_tag("ru").as_deref(), Some("ru"));
    assert_eq!(clean_tag("zh-Hans-CN").as_deref(), Some("zh-Hans-CN"));
    assert_eq!(clean_tag(""), None);
    assert_eq!(clean_tag("-array"), None);
    assert_eq!(clean_tag("ru; rm -rf"), None);
    assert_eq!(clean_tag("(ru)"), None);
  }

  #[test]
  fn camoufox_speaks_the_profile_language_unless_another_is_asked_for() {
    let german = fingerprint(&[("locale:language", "de"), ("locale:region", "de")]);
    assert_eq!(camoufox_profile_locale(&german).as_deref(), Some("de-DE"));
    let (name, value) = camoufox_env(&german, None).unwrap();
    assert_eq!(name, "CAMOU_PREFS_1");
    assert_eq!(value, r#"{"intl.locale.requested":"de-DE"}"#);

    let (_, asked) = camoufox_env(&german, Some("ru")).unwrap();
    assert_eq!(asked, r#"{"intl.locale.requested":"ru"}"#);
    // A value that is not a language tag is not passed on.
    let (_, ignored) = camoufox_env(&german, Some("ru\"}")).unwrap();
    assert_eq!(ignored, r#"{"intl.locale.requested":"de-DE"}"#);
  }

  #[test]
  fn a_fingerprint_without_a_language_sets_nothing() {
    assert_eq!(camoufox_env(&HashMap::new(), None), None);
    let bare = fingerprint(&[("locale:language", "ja")]);
    assert_eq!(camoufox_profile_locale(&bare).as_deref(), Some("ja"));
  }

  #[test]
  fn the_bundle_is_found_from_the_executable_inside_it() {
    let executable = Path::new("/bin/145/Chromium.app/Contents/MacOS/Chromium");
    assert_eq!(
      bundle_of(executable),
      Some(PathBuf::from("/bin/145/Chromium.app"))
    );
    assert_eq!(bundle_of(Path::new("/usr/bin/chromium")), None);
  }

  #[test]
  fn a_printed_preference_array_is_read_back_entry_by_entry() {
    assert_eq!(
      array_entries("(\n    \"de-DE\",\n    en\n)\n"),
      vec!["de-DE", "en"]
    );
    assert!(array_entries("(\n)\n").is_empty());
  }
}
