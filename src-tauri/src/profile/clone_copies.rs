//! Cloning a profile several times at once, and the fingerprint a clone must
//! not share with its source (fork).

use std::collections::HashSet;

use rand::RngExt;

use crate::camoufox_manager::CamoufoxConfig;
use crate::profile::{BrowserProfile, ProfileManager};

/// The most copies one request makes.
pub const MAX_COPIES: u32 = 50;

/// Fingerprint keys that are random draws of one device. A clone gets new
/// ones; the two noise seeds are left out so launch derives them from the
/// clone's own id (`camoufox::config::ensure_noise_seeds`).
const DERIVED_SEED_KEYS: [&str; 2] = ["canvas:seed", "audio:seed"];
const SPACING_SEED_KEY: &str = "fonts:spacing_seed";
const AA_OFFSET_KEY: &str = "canvas:aaOffset";
const SPACING_SEED_LIMIT: u32 = 1_073_741_824;
const AA_OFFSET_LIMIT: i32 = 50;

/// Names for `count` copies. One copy takes `base` when it is free; otherwise
/// the copies are `base 2`, `base 3`, ... skipping any name already in use.
pub fn copy_names(base: &str, count: u32, existing: &[String]) -> Vec<String> {
  let mut taken: HashSet<String> = existing.iter().map(|name| name.to_lowercase()).collect();
  if count == 1 && !taken.contains(&base.to_lowercase()) {
    return vec![base.to_string()];
  }
  let mut names = Vec::with_capacity(count as usize);
  let mut number: u64 = 2;
  while names.len() < count as usize {
    let candidate = format!("{base} {number}");
    number += 1;
    if taken.insert(candidate.to_lowercase()) {
      names.push(candidate);
    }
  }
  names
}

/// Give a cloned Camoufox config its own noise seeds, so the clone is not
/// linkable to its source by canvas, audio or font measurements. A config
/// that does not parse is left as it is.
pub fn refresh_camoufox_seeds(config: &mut CamoufoxConfig) {
  let Some(raw) = config.fingerprint.as_deref() else {
    return;
  };
  let Ok(mut values) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw)
  else {
    return;
  };
  for key in DERIVED_SEED_KEYS {
    values.remove(key);
  }
  let mut rng = rand::rng();
  if values.contains_key(SPACING_SEED_KEY) {
    values.insert(
      SPACING_SEED_KEY.to_string(),
      serde_json::json!(rng.random_range(0..SPACING_SEED_LIMIT)),
    );
  }
  if values.contains_key(AA_OFFSET_KEY) {
    values.insert(
      AA_OFFSET_KEY.to_string(),
      serde_json::json!(rng.random_range(-AA_OFFSET_LIMIT..=AA_OFFSET_LIMIT)),
    );
  }
  if let Ok(json) = serde_json::to_string(&values) {
    config.fingerprint = Some(json);
  }
}

/// Clone a profile `count` times (1 to `MAX_COPIES`). `name` is the base of
/// the copies' names; it defaults to the source's name.
#[tauri::command]
pub async fn clone_profile_copies(
  profile_id: String,
  name: Option<String>,
  count: u32,
) -> Result<Vec<BrowserProfile>, String> {
  if count == 0 || count > MAX_COPIES {
    return Err(
      serde_json::json!({
        "code": "CLONE_COUNT_INVALID",
        "params": { "max": MAX_COPIES.to_string() }
      })
      .to_string(),
    );
  }
  let joined = super::manager::run_blocking(move || clone_all(&profile_id, name, count)).await;
  joined.map_err(|e| crate::backend_error_with_detail("INTERNAL_ERROR", e))?
}

fn clone_all(
  profile_id: &str,
  name: Option<String>,
  count: u32,
) -> Result<Vec<BrowserProfile>, String> {
  let manager = ProfileManager::instance();
  let source = manager
    .find_profile(profile_id)
    .map_err(|e| crate::coded_failure("PROFILE_NOT_FOUND", e))?;
  let existing: Vec<String> = manager
    .list_profiles()
    .map_err(|e| crate::wrap_backend_error(e, "Failed to list profiles"))?
    .into_iter()
    .map(|profile| profile.name)
    .collect();
  let base = name
    .as_deref()
    .map(str::trim)
    .filter(|name| !name.is_empty())
    .unwrap_or(&source.name)
    .to_string();

  let names = copy_names(&base, count, &existing);
  let started = std::time::Instant::now();
  let mut created = Vec::with_capacity(names.len());
  for copy_name in names {
    match manager.clone_profile(profile_id, Some(copy_name)) {
      Ok(profile) => created.push(profile),
      Err(e) => {
        log::warn!(
          "Profile copies stopped profile={profile_id} done={} requested={count} err=\"{e}\"",
          created.len()
        );
        return Err(copy_failure(&e.to_string(), created.len(), count));
      }
    }
  }
  log::info!(
    "Profile copies made profile={profile_id} count={} elapsed_ms={}",
    created.len(),
    started.elapsed().as_millis()
  );
  Ok(created)
}

/// A coded error from below wins; anything else becomes `PROFILE_CLONE_FAILED`
/// with how far the batch got.
fn copy_failure(message: &str, done: usize, total: u32) -> String {
  if let Some(start) = message.find("{\"code\":") {
    return message[start..].to_string();
  }
  serde_json::json!({
    "code": "PROFILE_CLONE_FAILED",
    "params": {
      "detail": message,
      "done": done.to_string(),
      "total": total.to_string(),
    }
  })
  .to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
  }

  #[test]
  fn one_copy_takes_a_free_name_as_typed() {
    assert_eq!(copy_names("Work", 1, &names(&["Other"])), vec!["Work"]);
  }

  #[test]
  fn one_copy_of_a_taken_name_is_numbered_from_two() {
    assert_eq!(copy_names("Work", 1, &names(&["work"])), vec!["Work 2"]);
  }

  #[test]
  fn copies_are_numbered_from_two_and_skip_names_in_use() {
    let existing = names(&["Work", "Work 3", "WORK 5"]);
    assert_eq!(
      copy_names("Work", 4, &existing),
      vec!["Work 2", "Work 4", "Work 6", "Work 7"]
    );
  }

  #[test]
  fn fresh_seeds_replace_the_sources() {
    let mut config = CamoufoxConfig {
      fingerprint: Some(
        r#"{"canvas:seed":7,"audio:seed":9,"fonts:spacing_seed":123,"canvas:aaOffset":4,"navigator.userAgent":"UA"}"#
          .to_string(),
      ),
      ..Default::default()
    };
    refresh_camoufox_seeds(&mut config);
    let values: serde_json::Map<String, serde_json::Value> =
      serde_json::from_str(config.fingerprint.as_deref().unwrap()).unwrap();
    assert!(!values.contains_key("canvas:seed"));
    assert!(!values.contains_key("audio:seed"));
    assert_eq!(values["navigator.userAgent"], "UA");
    assert!(values["fonts:spacing_seed"].as_u64().unwrap() < u64::from(SPACING_SEED_LIMIT));
    assert!(values["canvas:aaOffset"].as_i64().unwrap().abs() <= 50);
  }

  #[test]
  fn an_unparsable_fingerprint_is_left_alone() {
    let mut config = CamoufoxConfig {
      fingerprint: Some("not json".to_string()),
      ..Default::default()
    };
    refresh_camoufox_seeds(&mut config);
    assert_eq!(config.fingerprint.as_deref(), Some("not json"));
  }

  #[test]
  fn a_coded_error_from_below_is_kept() {
    let coded = r#"{"code":"PROFILE_RUNNING"}"#;
    assert_eq!(copy_failure(&format!("x {coded}"), 1, 3), coded);
    let plain: serde_json::Value = serde_json::from_str(&copy_failure("disk full", 2, 5)).unwrap();
    assert_eq!(plain["code"], "PROFILE_CLONE_FAILED");
    assert_eq!(plain["params"]["done"], "2");
  }
}
