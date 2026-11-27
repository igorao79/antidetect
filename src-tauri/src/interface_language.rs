//! The language a profile's browser shows its own interface in (fork).
//!
//! Camoufox keeps no interface language apart from what a page can read, so
//! by default the interface follows the profile's own language and the two
//! cannot disagree. Measured (`s14_interface_language.py`): Camoufox takes
//! everything from `intl.locale.requested`: menus, `Intl`, date and number
//! formats, form messages. Left unset it stays `en-US` under any
//! `navigator.language`. Its translations ship inside the build.
//!
//! A profile may ask for another interface language (`interface_language`),
//! which a page can then notice through exactly those surfaces.
//!
//! The profile's own language (`language`) is this computer's unless the
//! profile names a tag, or `proxy` to follow its exit country. The language
//! replaces the exit's `locale:*` at every launch, while the timezone and
//! coordinates still follow the exit.

use serde_json::Value;
use std::collections::HashMap;

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

/// The `language` that names this computer's language, as unset does.
pub const SYSTEM_LANGUAGE: &str = "system";
/// The `language` that leaves the profile speaking its exit country's.
pub const PROXY_LANGUAGE: &str = "proxy";

/// The language tag a profile's `language` stands for at this launch: the
/// tag itself, none for `proxy` (follow the exit), and this computer's
/// language when unset or `system`.
pub fn resolve_profile_language(language: Option<&str>) -> Option<String> {
  match language.map(str::trim).filter(|value| !value.is_empty()) {
    Some(PROXY_LANGUAGE) => None,
    None | Some(SYSTEM_LANGUAGE) => sys_locale::get_locale().and_then(|locale| clean_tag(&locale)),
    Some(tag) => Some(tag.to_string()),
  }
}

/// Put a language the profile names, such as `ru-RU` or `zh-Hans-CN`, in
/// place of the fingerprint's `locale:*`. Nothing changes when the profile
/// names none or the tag is not one.
pub fn apply_profile_language(fingerprint: &mut HashMap<String, Value>, language: Option<&str>) {
  let Some(tag) = language.and_then(clean_tag) else {
    return;
  };
  let mut parts = tag.split('-');
  let Some(primary) = parts.next() else {
    return;
  };
  let mut script = None;
  let mut region = None;
  for part in parts {
    if part.len() == 4 && script.is_none() && region.is_none() {
      let mut letters = part.chars();
      script = letters.next().map(|first| {
        first.to_ascii_uppercase().to_string() + &letters.as_str().to_ascii_lowercase()
      });
    } else if (part.len() == 2 || part.len() == 3) && region.is_none() {
      region = Some(part.to_ascii_uppercase());
    }
  }
  let mut set = |key: &str, value: Option<String>| match value {
    Some(value) => {
      fingerprint.insert(key.to_string(), Value::String(value));
    }
    None => {
      fingerprint.remove(key);
    }
  };
  set("locale:language", Some(primary.to_ascii_lowercase()));
  set("locale:region", region);
  set("locale:script", script);
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
  fn a_named_language_replaces_the_exit_locale() {
    let mut located = fingerprint(&[
      ("locale:language", "de"),
      ("locale:region", "DE"),
      ("timezone", "Europe/Berlin"),
    ]);
    apply_profile_language(&mut located, Some("ru-ru"));
    assert_eq!(located["locale:language"], "ru");
    assert_eq!(located["locale:region"], "RU");
    assert_eq!(located["timezone"], "Europe/Berlin");
    assert_eq!(camoufox_profile_locale(&located).as_deref(), Some("ru-RU"));

    apply_profile_language(&mut located, Some("zh-hans-cn"));
    assert_eq!(located["locale:language"], "zh");
    assert_eq!(located["locale:script"], "Hans");
    assert_eq!(located["locale:region"], "CN");

    apply_profile_language(&mut located, Some("ja"));
    assert_eq!(located["locale:language"], "ja");
    assert!(!located.contains_key("locale:region"));
    assert!(!located.contains_key("locale:script"));
  }

  #[test]
  fn unset_or_system_is_this_computers_language_and_proxy_the_exits() {
    let computer = sys_locale::get_locale().and_then(|locale| clean_tag(&locale));
    assert_eq!(resolve_profile_language(None), computer);
    assert_eq!(resolve_profile_language(Some("  ")), computer);
    assert_eq!(resolve_profile_language(Some(SYSTEM_LANGUAGE)), computer);
    assert_eq!(resolve_profile_language(Some(PROXY_LANGUAGE)), None);
    assert_eq!(
      resolve_profile_language(Some("de-DE")).as_deref(),
      Some("de-DE")
    );
  }

  #[test]
  fn no_language_or_a_bad_tag_keeps_the_exit_locale() {
    let exit = fingerprint(&[("locale:language", "de"), ("locale:region", "DE")]);
    let mut kept = exit.clone();
    apply_profile_language(&mut kept, None);
    apply_profile_language(&mut kept, Some("ru; rm -rf"));
    apply_profile_language(&mut kept, Some(""));
    assert_eq!(kept, exit);
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
}
