//! The team's proxy generator (`proxygenerator`, a Telegram bot with a signed
//! API) as a source of stored proxies (fork). A member enters the key pair the
//! bot's `/keys` gives; generated proxies become ordinary stored proxies that
//! remember their generator id, and erasing one for good releases its port
//! there.

pub mod client;
mod credentials;

use crate::browser::ProxySettings;
use crate::proxy_manager::{StoredProxy, PROXY_MANAGER};
use client::{Credentials, GeneratedProxy, GeneratorOptions};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;

const OPTIONS_PATH: &str = "/api/v1/options";
const PROXIES_PATH: &str = "/api/v1/proxies";
const BULK_PATH: &str = "/api/v1/proxies/bulk";
const RELEASE_FAILED_EVENT: &str = "proxy-generator-release-failed";

#[derive(Debug, Clone, Serialize)]
pub struct GeneratorStatus {
  pub configured: bool,
  pub key_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
  #[default]
  Socks5,
  Http,
}

impl Protocol {
  fn proxy_type(self) -> &'static str {
    match self {
      Protocol::Socks5 => "socks5",
      Protocol::Http => "http",
    }
  }
}

/// What the generate form asks for.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenerateRequest {
  pub count: u32,
  pub countries_mode: Option<String>,
  pub countries: Vec<String>,
  pub ttl_minutes: Option<u32>,
  pub name: Option<String>,
  pub note: Option<String>,
  pub protocol: Protocol,
}

fn non_blank(text: &Option<String>) -> Option<String> {
  text
    .as_deref()
    .map(str::trim)
    .filter(|t| !t.is_empty())
    .map(str::to_string)
}

/// The request body the generator takes.
fn request_body(request: &GenerateRequest) -> Map<String, Value> {
  let mut body = Map::new();
  if request.count > 1 {
    body.insert("count".into(), Value::from(request.count));
  }
  let countries: Vec<String> = request
    .countries
    .iter()
    .map(|c| c.trim().to_lowercase())
    .filter(|c| !c.is_empty())
    .collect();
  let mode = request.countries_mode.as_deref().unwrap_or("any");
  if mode != "any" {
    body.insert("countries_mode".into(), Value::from(mode));
    body.insert("countries".into(), Value::from(countries));
  }
  if let Some(minutes) = request.ttl_minutes {
    body.insert("ttl_minutes".into(), Value::from(minutes));
  }
  if let Some(name) = non_blank(&request.name) {
    body.insert("name".into(), Value::from(name));
  }
  if let Some(note) = non_blank(&request.note) {
    body.insert("note".into(), Value::from(note));
  }
  body
}

/// `base`, or `base (2)`, `base (3)`… when a stored proxy has that name.
fn unique_name(base: &str, taken: &HashSet<String>) -> String {
  if !taken.contains(base) {
    return base.to_string();
  }
  (2..)
    .map(|n| format!("{base} ({n})"))
    .find(|name| !taken.contains(name))
    .unwrap_or_else(|| base.to_string())
}

fn require_credentials() -> Result<Credentials, String> {
  credentials::load()?.ok_or_else(|| crate::backend_error("PROXY_GENERATOR_NO_KEY"))
}

fn status_of(stored: Option<&Credentials>) -> GeneratorStatus {
  GeneratorStatus {
    configured: stored.is_some(),
    key_hint: stored.map(|c| credentials::key_hint(&c.key)),
  }
}

async fn fetch_options(credentials: &Credentials) -> Result<GeneratorOptions, String> {
  let value = client::call(credentials, Method::GET, OPTIONS_PATH, &[], None)
    .await
    .map_err(|f| f.to_backend_error())?;
  serde_json::from_value(value)
    .map_err(|e| crate::backend_error_with_detail("PROXY_GENERATOR_UNAVAILABLE", e))
}

#[tauri::command]
pub async fn proxy_generator_status() -> Result<GeneratorStatus, String> {
  Ok(status_of(credentials::load()?.as_ref()))
}

/// Keeps a key pair only once the generator accepts it.
#[tauri::command]
pub async fn proxy_generator_set_credentials(
  key: String,
  secret: String,
) -> Result<GeneratorStatus, String> {
  let pair = Credentials {
    key: key.trim().to_string(),
    secret: secret.trim().to_string(),
  };
  if pair.key.is_empty() || pair.secret.is_empty() {
    return Err(crate::backend_error("PROXY_GENERATOR_KEY_INVALID"));
  }
  fetch_options(&pair).await?;
  credentials::save(&pair)?;
  log::info!("Proxy generator key saved");
  Ok(status_of(Some(&pair)))
}

#[tauri::command]
pub async fn proxy_generator_clear_credentials() -> Result<(), String> {
  credentials::clear()?;
  log::info!("Proxy generator key removed");
  Ok(())
}

#[tauri::command]
pub async fn proxy_generator_options() -> Result<GeneratorOptions, String> {
  fetch_options(&require_credentials()?).await
}

fn parse_generated(value: Value, bulk: bool) -> Result<Vec<GeneratedProxy>, String> {
  let parsed = if bulk {
    serde_json::from_value::<Vec<GeneratedProxy>>(value["items"].clone())
  } else {
    serde_json::from_value::<GeneratedProxy>(value).map(|p| vec![p])
  };
  parsed.map_err(|e| crate::backend_error_with_detail("PROXY_GENERATOR_UNAVAILABLE", e))
}

/// Saves one generated proxy as a stored proxy. A half-made one is removed
/// again, so nothing is left without its generator id.
fn store_generated(
  app_handle: &tauri::AppHandle,
  generated: &GeneratedProxy,
  protocol: Protocol,
) -> Result<StoredProxy, String> {
  let taken: HashSet<String> = PROXY_MANAGER
    .get_stored_proxies()
    .into_iter()
    .map(|p| p.name)
    .collect();
  let base =
    non_blank(&generated.name).unwrap_or_else(|| format!("{}:{}", generated.host, generated.port));
  let settings = ProxySettings {
    proxy_type: protocol.proxy_type().to_string(),
    host: generated.host.clone(),
    port: generated.port,
    username: Some(generated.username.clone()),
    password: Some(generated.password.clone()),
    vless_uri: None,
  };
  let created =
    PROXY_MANAGER.create_stored_proxy(app_handle, unique_name(&base, &taken), settings)?;
  let completed = PROXY_MANAGER
    .set_generator_id(&created.id, generated.id.clone())
    .and_then(|p| match generated.expires_at_secs() {
      Some(at) => PROXY_MANAGER.set_expires_at(&p.id, Some(at)),
      None => Ok(p),
    })
    .and_then(|p| match non_blank(&generated.note) {
      Some(note) => PROXY_MANAGER.set_note(&p.id, Some(note)),
      None => Ok(p),
    });
  if completed.is_err() {
    if let Err(e) = PROXY_MANAGER.delete_stored_proxy(app_handle, &created.id) {
      log::warn!(
        "Half-made generated proxy not removed proxy={} err=\"{e}\"",
        created.id
      );
    }
  }
  completed
}

/// Asks the generator for proxies and stores them. A proxy that cannot be
/// stored is released again; the rest are kept and returned.
#[tauri::command]
pub async fn proxy_generator_generate(
  app_handle: tauri::AppHandle,
  request: GenerateRequest,
) -> Result<Vec<StoredProxy>, String> {
  let credentials = require_credentials()?;
  let count = request.count.max(1);
  let bulk = count > 1;
  let request = GenerateRequest { count, ..request };
  let started = std::time::Instant::now();
  let value = client::call(
    &credentials,
    Method::POST,
    if bulk { BULK_PATH } else { PROXIES_PATH },
    &[],
    Some(request_body(&request)),
  )
  .await
  .map_err(|f| f.to_backend_error())?;
  let generated = parse_generated(value, bulk)?;

  let mut stored = Vec::with_capacity(generated.len());
  let mut first_error = None;
  for proxy in &generated {
    match store_generated(&app_handle, proxy, request.protocol) {
      Ok(saved) => stored.push(saved),
      Err(e) => {
        log::warn!(
          "Generated proxy not stored, releasing generator_id={} port={} err=\"{e}\"",
          proxy.id,
          proxy.port
        );
        release(&credentials, &proxy.id).await;
        first_error.get_or_insert(e);
      }
    }
  }
  log::info!(
    "Proxies generated requested={count} stored={} elapsed_ms={}",
    stored.len(),
    started.elapsed().as_millis()
  );
  match first_error {
    Some(e) if stored.is_empty() => Err(crate::coded_failure("PROXY_SAVE_FAILED", e)),
    _ => Ok(stored),
  }
}

/// Deletes the generator's entry; one that is already gone counts as
/// released. Returns whether it is released.
async fn release(credentials: &Credentials, generator_id: &str) -> bool {
  let path = format!("{PROXIES_PATH}/{generator_id}");
  match client::call(credentials, Method::DELETE, &path, &[], Some(Map::new())).await {
    Ok(_) => true,
    Err(failure) if failure.is_not_found() => true,
    Err(failure) => {
      log::warn!(
        "Generator proxy not released generator_id={generator_id} err=\"{}\"",
        failure.to_backend_error()
      );
      false
    }
  }
}

fn is_plain_id(id: &str) -> bool {
  !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Releases, in the background, the generator entry of a proxy erased for
/// good. The interface hears of a failure through
/// `proxy-generator-release-failed`.
pub fn release_later(proxy_id: String, generator_id: String) {
  if !is_plain_id(&generator_id) {
    return;
  }
  tauri::async_runtime::spawn(async move {
    let released = match credentials::load() {
      Ok(Some(credentials)) => release(&credentials, &generator_id).await,
      Ok(None) => {
        log::warn!(
          "Generator proxy not released, no key proxy={proxy_id} generator_id={generator_id}"
        );
        false
      }
      Err(e) => {
        log::warn!("Generator key unreadable proxy={proxy_id} err=\"{e}\"");
        false
      }
    };
    if released {
      log::info!("Generator proxy released proxy={proxy_id} generator_id={generator_id}");
      return;
    }
    if let Err(e) = crate::events::emit(
      RELEASE_FAILED_EVENT,
      serde_json::json!({ "proxyId": proxy_id }),
    ) {
      log::warn!("Event emit failed event={RELEASE_FAILED_EVENT} err=\"{e}\"");
    }
  });
}

/// `release_later` for a proxy stored as JSON (the trash keeps it that way).
pub fn release_stored_json(entity: &Value) {
  if let (Some(id), Some(generator_id)) = (entity["id"].as_str(), entity["generator_id"].as_str()) {
    release_later(id.to_string(), generator_id.to_string());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_body_carries_only_what_the_form_set() {
    let single = request_body(&GenerateRequest {
      count: 1,
      name: Some("  ".into()),
      ..Default::default()
    });
    assert!(single.is_empty());

    let batch = request_body(&GenerateRequest {
      count: 3,
      countries_mode: Some("exclude".into()),
      countries: vec![" RU".into(), "by".into(), String::new()],
      ttl_minutes: Some(90),
      name: Some("kraken".into()),
      note: Some(" reg ".into()),
      protocol: Protocol::Http,
    });
    assert_eq!(
      Value::Object(batch),
      serde_json::json!({
        "count": 3, "countries_mode": "exclude", "countries": ["ru", "by"],
        "ttl_minutes": 90, "name": "kraken", "note": "reg"
      })
    );
  }

  #[test]
  fn a_taken_name_gets_the_next_free_number() {
    let taken: HashSet<String> = ["a".to_string(), "a (2)".to_string()].into();
    assert_eq!(unique_name("a", &taken), "a (3)");
    assert_eq!(unique_name("b", &taken), "b");
  }

  #[test]
  fn a_bulk_answer_and_a_single_one_both_parse() {
    let proxy = serde_json::json!({
      "id": "6f1c", "host": "gw.dataimpulse.com", "port": 15000,
      "username": "u", "password": "p", "name": null, "expires_at": null
    });
    assert_eq!(parse_generated(proxy.clone(), false).unwrap().len(), 1);
    let bulk = serde_json::json!({ "items": [proxy.clone(), proxy] });
    assert_eq!(parse_generated(bulk, true).unwrap().len(), 2);
  }

  #[test]
  fn only_a_plain_generator_id_reaches_a_url() {
    assert!(is_plain_id("3f2a9c1e-0b7d-4e57-9a8e-1c2d3e4f5a6b"));
    assert!(!is_plain_id("../options"));
    assert!(!is_plain_id(""));
  }

  #[test]
  fn the_protocol_names_the_stored_type() {
    let request: GenerateRequest = serde_json::from_value(serde_json::json!({
      "count": 1, "protocol": "http", "countriesMode": "only", "countries": ["us"], "ttlMinutes": 60
    }))
    .unwrap();
    assert_eq!(request.protocol.proxy_type(), "http");
    assert_eq!(request.ttl_minutes, Some(60));
    assert_eq!(GenerateRequest::default().protocol.proxy_type(), "socks5");
  }
}
