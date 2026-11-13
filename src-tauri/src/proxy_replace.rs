//! Moving every profile that uses one stored proxy onto another in one step
//! (fork). Each profile goes through `ProfileManager::update_profile_proxy`, so
//! the usual sync queueing, team report, `updated_at` bump and change events
//! happen exactly as for a single edit.

use crate::profile::types::BrowserProfile;

/// Ids of the profiles whose `proxy_id` is `from`.
fn profile_ids_using(profiles: &[BrowserProfile], from: &str) -> Vec<String> {
  profiles
    .iter()
    .filter(|profile| profile.proxy_id.as_deref() == Some(from))
    .map(|profile| profile.id.to_string())
    .collect()
}

/// Check the pair before anything moves: a target that is not stored, or the
/// same proxy twice, changes nothing and is refused.
fn validate_pair(from: &str, to: &str, target_exists: bool) -> Result<(), String> {
  if from == to {
    return Err(crate::backend_error("PROXY_REPLACE_SAME"));
  }
  if !target_exists {
    return Err(crate::backend_error("PROXY_NOT_FOUND"));
  }
  Ok(())
}

pub async fn replace_proxy_in_profiles(
  app_handle: tauri::AppHandle,
  from: String,
  to: String,
) -> Result<usize, String> {
  let target_exists = crate::proxy_manager::PROXY_MANAGER
    .get_proxy_settings_by_id(&to)
    .is_some();
  validate_pair(&from, &to, target_exists)?;

  let manager = crate::profile::ProfileManager::instance();
  let profiles = manager
    .list_profiles()
    .map_err(|e| crate::coded_failure("PROXY_REPLACE_FAILED", e))?;
  let ids = profile_ids_using(&profiles, &from);

  let mut moved = 0usize;
  for id in &ids {
    manager
      .update_profile_proxy(app_handle.clone(), id, Some(to.clone()))
      .await
      .map_err(|e| crate::coded_failure("PROXY_REPLACE_FAILED", e))?;
    moved += 1;
  }
  log::info!(
    "Proxy replaced in profiles from={from} to={to} profiles={moved} total={}",
    ids.len()
  );
  Ok(moved)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn same_proxy_is_refused() {
    let err = validate_pair("a", "a", true).unwrap_err();
    assert!(err.contains("PROXY_REPLACE_SAME"));
  }

  #[test]
  fn missing_target_is_refused() {
    let err = validate_pair("a", "b", false).unwrap_err();
    assert!(err.contains("PROXY_NOT_FOUND"));
  }

  #[test]
  fn valid_pair_passes() {
    assert!(validate_pair("a", "b", true).is_ok());
  }

  #[test]
  fn only_profiles_on_the_source_proxy_are_picked() {
    let on_a = BrowserProfile {
      proxy_id: Some("a".into()),
      ..Default::default()
    };
    let on_b = BrowserProfile {
      proxy_id: Some("b".into()),
      ..Default::default()
    };
    let none = BrowserProfile::default();
    let ids = profile_ids_using(&[on_a.clone(), on_b, none], "a");
    assert_eq!(ids, vec![on_a.id.to_string()]);
  }
}
