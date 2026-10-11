//! Camoufox pieces of profile creation.

use crate::camoufox_manager::CamoufoxConfig;
use crate::proxy_manager::PROXY_MANAGER;
use serde_json::Value;

/// How long a successful proxy check answers for the next profile. A series of
/// profiles on one proxy is created seconds apart, so they share a measurement
/// instead of taking one each. Kept short: a proxy that has just died should
/// still refuse the next profile.
const EXIT_REUSE_SECS: u64 = 15;

/// The config a new profile's fingerprint is generated from: the user's own,
/// located at the exit of the proxy the profile will use.
///
/// The exit is measured the way a proxy check measures it, through a local
/// worker. Handing the proxy to the HTTP client instead only works for the
/// types that client speaks; for the rest (SSH, Shadowsocks, share links) it
/// looks up this machine's own address without saying so, and the fingerprint
/// is located in the wrong place. A proxy that cannot be reached fails the
/// creation with the check's own error.
pub(super) async fn config_for_generation(
  config: &CamoufoxConfig,
  proxy_id: Option<&str>,
) -> Result<CamoufoxConfig, String> {
  let mut generation = config.clone();
  generation.proxy = None;
  if !matches!(config.geoip, None | Some(Value::Bool(true))) {
    return Ok(generation);
  }
  let Some(proxy_id) = proxy_id else {
    return Ok(generation);
  };
  let Some(settings) = PROXY_MANAGER.get_proxy_settings_by_id(proxy_id) else {
    return Ok(generation);
  };
  let recent = PROXY_MANAGER
    .get_cached_proxy_check(proxy_id)
    .filter(|check| {
      check.is_valid
        && !check.ip.is_empty()
        && crate::proxy_manager::now_secs().saturating_sub(check.timestamp) <= EXIT_REUSE_SECS
    });
  let exit_ip = match recent {
    Some(check) => check.ip,
    None => {
      PROXY_MANAGER
        .check_proxy_validity(proxy_id, &settings)
        .await?
        .ip
    }
  };
  generation.geoip = Some(Value::String(exit_ip));
  Ok(generation)
}
