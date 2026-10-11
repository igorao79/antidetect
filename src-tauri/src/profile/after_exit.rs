//! What a profile is owed once its browser has gone, for the engines
//! Antidetect adds. Wayfern does the same inline in `browser_runner.rs`.

use crate::profile::{BrowserProfile, ProfileManager};

/// Clean up after `profile`'s browser, whether the app stopped it or the user
/// closed its window: drop the in-memory data of an ephemeral profile, clear
/// the browsing data of one set to clear on close, remove unpacked extension
/// code, and delete a temporary profile for good.
pub(crate) async fn settle(app_handle: &tauri::AppHandle, profile: &BrowserProfile) {
  let id = profile.id.to_string();
  if profile.ephemeral && !profile.password_protected {
    crate::ephemeral_dirs::remove_ephemeral_dir(&id);
  }
  crate::profile::clear_on_close::clear_profile_browsing_data(profile).await;
  crate::extension_manager::ExtensionManager::cleanup_unpacked_for_profile(&id);

  if profile.temporary {
    match ProfileManager::instance().delete_profile_permanently(app_handle, &id) {
      Ok(()) => log::info!(
        "Deleted temporary profile {} now that its browser has stopped",
        profile.name
      ),
      Err(e) => log::warn!("Could not delete temporary profile {}: {e}", profile.name),
    }
  }
}
