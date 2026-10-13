//! Profile templates (fork): named sets of the create dialog's choices.
//!
//! A template is what the dialog would send for a new profile, minus the name
//! and anything that makes one device (a pinned fingerprint or seed). The
//! settings are kept as the frontend wrote them; this module only stores them.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_TEMPLATES: usize = 100;
const MAX_NAME_CHARS: usize = 80;

/// Serialises read-modify-write of the one templates file.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileTemplate {
  pub id: String,
  pub name: String,
  pub settings: serde_json::Value,
  pub updated_at: u64,
}

fn error(code: &str) -> String {
  serde_json::json!({ "code": code }).to_string()
}

fn templates_file() -> PathBuf {
  crate::app_dirs::data_subdir().join("profile_templates.json")
}

fn load(file: &Path) -> Vec<ProfileTemplate> {
  let Ok(content) = std::fs::read_to_string(file) else {
    return Vec::new();
  };
  serde_json::from_str(&content).unwrap_or_else(|e| {
    log::warn!("Profile templates could not be read, starting empty: {e}");
    Vec::new()
  })
}

fn store(file: &Path, templates: &[ProfileTemplate]) -> std::io::Result<()> {
  if let Some(dir) = file.parent() {
    std::fs::create_dir_all(dir)?;
  }
  let staged = file.with_extension("json.tmp");
  std::fs::write(&staged, serde_json::to_vec_pretty(templates)?)?;
  std::fs::rename(&staged, file)
}

/// Saves `settings` under `name`, replacing the template that already has
/// that name, and returns the list as it now stands, by name.
fn save_into(
  file: &Path,
  name: &str,
  settings: serde_json::Value,
) -> Result<Vec<ProfileTemplate>, String> {
  let name = name.trim();
  if name.is_empty() {
    return Err(error("NAME_CANNOT_BE_EMPTY"));
  }
  if name.chars().count() > MAX_NAME_CHARS || !settings.is_object() {
    return Err(error("PROFILE_TEMPLATE_INVALID"));
  }
  let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  let mut templates = load(file);
  let updated_at = crate::proxy_manager::now_secs();
  match templates.iter_mut().find(|template| template.name == name) {
    Some(existing) => {
      existing.settings = settings;
      existing.updated_at = updated_at;
    }
    None => {
      if templates.len() >= MAX_TEMPLATES {
        return Err(error("PROFILE_TEMPLATE_LIMIT"));
      }
      templates.push(ProfileTemplate {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        settings,
        updated_at,
      });
    }
  }
  templates.sort_by_key(|template| template.name.to_lowercase());
  store(file, &templates)
    .map_err(|e| crate::wrap_backend_error(e, "Failed to save the template"))?;
  Ok(templates)
}

fn delete_from(file: &Path, id: &str) -> Result<Vec<ProfileTemplate>, String> {
  let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  let mut templates = load(file);
  let before = templates.len();
  templates.retain(|template| template.id != id);
  if templates.len() == before {
    return Err(error("PROFILE_TEMPLATE_NOT_FOUND"));
  }
  store(file, &templates)
    .map_err(|e| crate::wrap_backend_error(e, "Failed to delete the template"))?;
  Ok(templates)
}

#[tauri::command]
pub async fn list_profile_templates() -> Result<Vec<ProfileTemplate>, String> {
  Ok(load(&templates_file()))
}

#[tauri::command]
pub async fn save_profile_template(
  name: String,
  settings: serde_json::Value,
) -> Result<Vec<ProfileTemplate>, String> {
  save_into(&templates_file(), &name, settings)
}

#[tauri::command]
pub async fn delete_profile_template(id: String) -> Result<Vec<ProfileTemplate>, String> {
  delete_from(&templates_file(), &id)
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data").join("profile_templates.json");
    (dir, file)
  }

  #[test]
  fn a_template_is_kept_listed_by_name_and_deleted() {
    let (_dir, file) = file();
    save_into(&file, "Windows shop", json!({ "engine": "camoufox" })).unwrap();
    let listed = save_into(&file, " ads ", json!({ "engine": "cloakbrowser" })).unwrap();
    assert_eq!(
      listed.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
      vec!["ads", "Windows shop"]
    );
    assert_eq!(load(&file), listed);

    let left = delete_from(&file, &listed[0].id).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "Windows shop");
    assert!(delete_from(&file, "no-such-id")
      .unwrap_err()
      .contains("PROFILE_TEMPLATE_NOT_FOUND"));
  }

  #[test]
  fn saving_under_a_name_in_use_replaces_that_template() {
    let (_dir, file) = file();
    let first = save_into(&file, "Shop", json!({ "engine": "camoufox" })).unwrap();
    let second = save_into(&file, "Shop", json!({ "engine": "cloakbrowser" })).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, first[0].id);
    assert_eq!(second[0].settings["engine"], "cloakbrowser");
  }

  #[test]
  fn a_template_needs_a_name_and_settings() {
    let (_dir, file) = file();
    assert!(save_into(&file, "  ", json!({}))
      .unwrap_err()
      .contains("NAME_CANNOT_BE_EMPTY"));
    assert!(save_into(&file, "Shop", json!("not settings"))
      .unwrap_err()
      .contains("PROFILE_TEMPLATE_INVALID"));
    assert!(save_into(&file, &"n".repeat(MAX_NAME_CHARS + 1), json!({}))
      .unwrap_err()
      .contains("PROFILE_TEMPLATE_INVALID"));
    assert!(load(&file).is_empty());
  }

  #[test]
  fn an_unreadable_file_is_an_empty_list_not_a_failure() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "not json").unwrap();
    assert!(load(&file).is_empty());
  }
}
