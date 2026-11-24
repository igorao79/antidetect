//! Removing a stored proxy once its expiry date has passed (fork).
//!
//! `StoredProxy.expires_at` is the end of the person's plan for a proxy: the
//! end of the day it names. The date used to be a mark on the profiles using
//! it; a date that has passed now removes the proxy, so a temporary route does
//! not stay in the list after its time. Every profile it served is detached
//! first, so its cell shows the unset dash and another proxy can be assigned.
//!
//! The removal goes through the entity trash like a delete from the Network
//! page, which keeps a wrongly set date recoverable; `entity_trash` brings a
//! restored proxy back without the past date, or the next sweep would remove
//! it again at once.

use std::time::Duration;

use crate::entity_trash::{trash_entity, EntityKind};
use crate::log_streak::KeyedStreak;
use crate::profile::ProfileManager;
use crate::proxy_manager::{now_secs, StoredProxy, PROXY_MANAGER};

/// How often the store is looked through. Short enough that a profile loses
/// its proxy seconds after the date passes, and one pass is a list read.
const SWEEP_INTERVAL: Duration = Duration::from_secs(10);

static DETACH: KeyedStreak = KeyedStreak::new(module_path!(), "Expired proxy not detached");
static REMOVE: KeyedStreak = KeyedStreak::new(module_path!(), "Expired proxy not removed");

/// Whether the proxy's date has passed. A cloud proxy's configuration is the
/// cloud's, so its date never removes it here.
fn expired(proxy: &StoredProxy, now: u64) -> bool {
  !proxy.is_cloud_managed
    && !proxy.is_cloud_derived
    && proxy.expires_at.is_some_and(|at| at > 0 && at <= now)
}

fn expired_ids(proxies: &[StoredProxy], now: u64) -> Vec<String> {
  proxies
    .iter()
    .filter(|proxy| expired(proxy, now))
    .map(|proxy| proxy.id.clone())
    .collect()
}

/// Start the sweep: once the startup scan is done, then every
/// `SWEEP_INTERVAL`. The scan writes profile metadata (stale PIDs), so
/// detaching before it finishes could be overwritten by its copy.
pub fn start_sweeper(app_handle: tauri::AppHandle) {
  tauri::async_runtime::spawn(async move {
    crate::STARTUP_SCAN_DONE.wait().await;
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
      interval.tick().await;
      sweep(&app_handle).await;
    }
  });
}

async fn sweep(app_handle: &tauri::AppHandle) {
  let now = now_secs();
  for proxy_id in expired_ids(&PROXY_MANAGER.get_stored_proxies(), now) {
    remove(app_handle, &proxy_id, now).await;
  }
}

/// Detach every profile from the proxy, then move the proxy to the trash. A
/// profile left pointing at a removed proxy could not launch at all
/// (`PROFILE_PROXY_MISSING`), instead of letting the person pick another.
async fn remove(app_handle: &tauri::AppHandle, proxy_id: &str, now: u64) {
  // The person may have moved the date since the store was read.
  let still_expired = PROXY_MANAGER
    .get_stored_proxies()
    .into_iter()
    .find(|proxy| proxy.id == proxy_id)
    .is_some_and(|proxy| expired(&proxy, now));
  if !still_expired {
    return;
  }

  let manager = ProfileManager::instance();
  let profiles = match manager.list_profiles() {
    Ok(profiles) => profiles,
    Err(e) => {
      REMOVE.failed(&format!("proxy={proxy_id}"), e);
      return;
    }
  };
  let users: Vec<String> = profiles
    .iter()
    .filter(|profile| profile.proxy_id.as_deref() == Some(proxy_id))
    .map(|profile| profile.id.to_string())
    .collect();

  for profile_id in &users {
    let key = format!("profile={profile_id}");
    if let Err(e) = manager
      .update_profile_proxy(app_handle.clone(), profile_id, None)
      .await
    {
      // The proxy stays for the next pass: taking it now would leave this
      // profile pointing at nothing.
      DETACH.failed(&key, e);
      return;
    }
    DETACH.succeeded(&key);
  }

  let key = format!("proxy={proxy_id}");
  match trash_entity(app_handle.clone(), EntityKind::Proxy, proxy_id.to_string()).await {
    Ok(_) => {
      REMOVE.succeeded(&key);
      log::info!(
        "Expired proxy removed proxy={proxy_id} profiles={}",
        users.len()
      );
    }
    // Another sweep, or another device's sync, already removed it.
    Err(e) if e.contains("PROXY_NOT_FOUND") => REMOVE.succeeded(&key),
    Err(e) => REMOVE.failed(&key, e),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::browser::ProxySettings;

  const NOW: u64 = 1_800_000_000;

  fn proxy_with(expires_at: Option<u64>) -> StoredProxy {
    let mut proxy = StoredProxy::new(
      "route".to_string(),
      ProxySettings {
        proxy_type: "socks5".to_string(),
        host: "127.0.0.1".to_string(),
        port: 1080,
        username: None,
        password: None,
        vless_uri: None,
      },
    );
    proxy.expires_at = expires_at;
    proxy
  }

  #[test]
  fn only_a_date_that_has_passed_expires() {
    assert!(!expired(&proxy_with(None), NOW), "no date, no timer");
    assert!(!expired(&proxy_with(Some(0)), NOW), "0 is not a date");
    assert!(!expired(&proxy_with(Some(NOW + 1)), NOW));
    assert!(expired(&proxy_with(Some(NOW)), NOW));
    assert!(expired(&proxy_with(Some(NOW - 1)), NOW));
  }

  #[test]
  fn a_cloud_proxy_is_left_to_the_cloud() {
    let mut managed = proxy_with(Some(NOW - 1));
    managed.is_cloud_managed = true;
    assert!(!expired(&managed, NOW));

    let mut derived = proxy_with(Some(NOW - 1));
    derived.is_cloud_derived = true;
    assert!(!expired(&derived, NOW));
  }

  #[test]
  fn a_sweep_takes_only_the_expired_ids() {
    let gone = proxy_with(Some(NOW - 10));
    let soon = proxy_with(Some(NOW + 10));
    assert_eq!(
      expired_ids(&[gone.clone(), soon], NOW),
      vec![gone.id.clone()]
    );
  }
}
