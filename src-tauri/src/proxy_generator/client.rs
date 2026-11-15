//! The proxy generator's signed HTTP API (`proxygenerator`, the team's own
//! service; its contract is its `docs/api.md`). Signature scheme v2: hex
//! HMAC-SHA256(secret, canonical JSON of `{v: 2, method, path, query, body}`)
//! with keys sorted at every level and `_timestamp` in milliseconds, in the
//! query of a GET and in the body of everything else.

use reqwest::Method;
use ring::hmac;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::Duration;

const PRODUCTION_BASE: &str = "https://telemanager.online/proxygen";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The key pair a member gets from the generator's bot (`/keys`).
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Credentials {
  pub key: String,
  pub secret: String,
}

/// What `GET /api/v1/options` tells the form.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratorOptions {
  pub enabled: bool,
  pub countries: Vec<String>,
  pub popular_countries: Vec<String>,
  pub protocols: Vec<String>,
  pub ttl_hours: Vec<u32>,
  pub ttl_minutes_min: u32,
  pub ttl_minutes_max: u32,
  pub bulk_max: u32,
  pub owned_max: u32,
}

/// One proxy as the generator hands it out.
#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedProxy {
  pub id: String,
  pub host: String,
  pub port: u16,
  pub username: String,
  pub password: String,
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default)]
  pub note: Option<String>,
  /// ISO 8601; `None` for a proxy without an end.
  #[serde(default)]
  pub expires_at: Option<String>,
}

impl GeneratedProxy {
  /// The end as unix seconds; an unreadable date counts as none.
  pub fn expires_at_secs(&self) -> Option<u64> {
    let text = self.expires_at.as_deref()?;
    chrono::DateTime::parse_from_rfc3339(text)
      .ok()
      .and_then(|at| u64::try_from(at.timestamp()).ok())
  }
}

/// Why a call failed.
#[derive(Debug)]
pub enum Failure {
  /// The request never got an answer; the coded error says why.
  Network(String),
  /// The generator answered with an error.
  Api {
    status: u16,
    code: String,
    message: String,
  },
}

impl Failure {
  pub fn is_not_found(&self) -> bool {
    matches!(self, Failure::Api { code, .. } if code == "PROXY_NOT_FOUND")
  }

  /// The Owl error code the interface translates.
  pub fn to_backend_error(&self) -> String {
    match self {
      Failure::Network(coded) => coded.clone(),
      Failure::Api {
        status,
        code,
        message,
      } => match code.as_str() {
        "MISSING_API_KEY" | "INVALID_API_KEY" | "MISSING_SIGNATURE" | "INVALID_SIGNATURE" => {
          crate::backend_error("PROXY_GENERATOR_KEY_INVALID")
        }
        "TIMESTAMP_SKEW" => crate::backend_error("PROXY_GENERATOR_CLOCK_SKEW"),
        "PROXY_LIMIT" => crate::backend_error("PROXY_GENERATOR_LIMIT"),
        "PROXY_POOL_EXHAUSTED" => crate::backend_error("PROXY_GENERATOR_POOL_EXHAUSTED"),
        "RATE_LIMITED" => crate::backend_error("PROXY_GENERATOR_RATE_LIMITED"),
        _ if *status == 429 => crate::backend_error("PROXY_GENERATOR_RATE_LIMITED"),
        _ if *status >= 500 => {
          crate::backend_error_with_detail("PROXY_GENERATOR_UNAVAILABLE", message)
        }
        _ => crate::backend_error_with_detail("PROXY_GENERATOR_REJECTED", message),
      },
    }
  }
}

/// Where the API lives. Debug builds take `OWL_PROXYGEN_URL`, so the E2E
/// suites can point the app at a local stand-in.
pub fn base_url() -> String {
  if cfg!(debug_assertions) {
    if let Ok(url) = std::env::var("OWL_PROXYGEN_URL") {
      let url = url.trim().trim_end_matches('/');
      if !url.is_empty() {
        return url.to_string();
      }
    }
  }
  PRODUCTION_BASE.to_string()
}

/// JSON with object keys sorted at every level and no spaces: what the
/// generator's `JSON.stringify(sortKeysDeep(payload))` produces.
pub fn canonical_json(value: &Value) -> String {
  match value {
    Value::Object(map) => {
      let mut keys: Vec<&String> = map.keys().collect();
      keys.sort();
      let fields: Vec<String> = keys
        .into_iter()
        .map(|key| {
          format!(
            "{}:{}",
            Value::String(key.clone()),
            canonical_json(&map[key])
          )
        })
        .collect();
      format!("{{{}}}", fields.join(","))
    }
    Value::Array(items) => {
      let items: Vec<String> = items.iter().map(canonical_json).collect();
      format!("[{}]", items.join(","))
    }
    other => other.to_string(),
  }
}

pub fn sign(secret: &str, payload: &Value) -> String {
  let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
  let tag = hmac::sign(&key, canonical_json(payload).as_bytes());
  tag.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The signed parts of a request: the query and body as sent, and the
/// signature over them.
pub struct SignedRequest {
  pub query: Map<String, Value>,
  pub body: Map<String, Value>,
  pub signature: String,
}

pub fn sign_request(
  secret: &str,
  method: &Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Map<String, Value>>,
  timestamp_ms: u64,
) -> SignedRequest {
  let is_get = *method == Method::GET;
  let mut signed_query = Map::new();
  let mut signed_body = Map::new();
  if is_get {
    for (key, value) in query {
      signed_query.insert((*key).to_string(), Value::String(value.clone()));
    }
    signed_query.insert(
      "_timestamp".to_string(),
      Value::String(timestamp_ms.to_string()),
    );
  } else {
    signed_body = body.unwrap_or_default();
    signed_body.insert("_timestamp".to_string(), Value::from(timestamp_ms));
  }
  let payload = serde_json::json!({
    "v": 2,
    "method": method.as_str(),
    "path": path,
    "query": signed_query,
    "body": signed_body,
  });
  SignedRequest {
    signature: sign(secret, &payload),
    query: signed_query,
    body: signed_body,
  }
}

fn now_ms() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    .unwrap_or_default()
}

fn network_failure(error: &reqwest::Error) -> Failure {
  Failure::Network(
    crate::system_proxy::explain(error)
      .unwrap_or_else(|| crate::backend_error_with_detail("PROXY_GENERATOR_UNAVAILABLE", error)),
  )
}

/// The result of a successful answer, or what went wrong.
fn read_envelope(status: u16, text: &str) -> Result<Value, Failure> {
  let parsed: Option<Value> = serde_json::from_str(text).ok();
  match parsed {
    Some(envelope) if envelope["ok"] == Value::Bool(true) => Ok(envelope["result"].clone()),
    Some(envelope) if envelope["ok"] == Value::Bool(false) => Err(Failure::Api {
      status,
      code: envelope["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string(),
      message: envelope["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string(),
    }),
    _ => Err(Failure::Api {
      status,
      code: String::new(),
      message: format!("HTTP {status}"),
    }),
  }
}

/// One signed call. `path` is the service's own path (`/api/v1/...`).
pub async fn call(
  credentials: &Credentials,
  method: Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Map<String, Value>>,
) -> Result<Value, Failure> {
  call_at(&base_url(), credentials, method, path, query, body).await
}

async fn call_at(
  base: &str,
  credentials: &Credentials,
  method: Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Map<String, Value>>,
) -> Result<Value, Failure> {
  let signed = sign_request(&credentials.secret, &method, path, query, body, now_ms());
  let client = reqwest::Client::builder()
    .timeout(REQUEST_TIMEOUT)
    .build()
    .unwrap_or_default();
  let mut url = url::Url::parse(&format!("{base}{path}")).map_err(|e| {
    Failure::Network(crate::backend_error_with_detail(
      "PROXY_GENERATOR_UNAVAILABLE",
      e,
    ))
  })?;
  if !signed.query.is_empty() {
    let mut pairs = url.query_pairs_mut();
    for (key, value) in &signed.query {
      pairs.append_pair(key, value.as_str().unwrap_or_default());
    }
  }
  let mut request = client
    .request(method.clone(), url)
    .header("X-Store-Key", credentials.key.as_str())
    .header("X-Signature", signed.signature.as_str());
  if method != Method::GET {
    request = request
      .header("Content-Type", "application/json")
      .body(Value::Object(signed.body).to_string());
  }
  let response = request.send().await.map_err(|e| network_failure(&e))?;
  let status = response.status().as_u16();
  let text = response.text().await.map_err(|e| network_failure(&e))?;
  read_envelope(status, &text)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn canonical_json_sorts_keys_at_every_level_and_keeps_unicode() {
    let value =
      serde_json::json!({ "b": [{ "z": 1, "a": "кот" }], "a": { "y": null, "x": "q\"" } });
    assert_eq!(
      canonical_json(&value),
      r#"{"a":{"x":"q\"","y":null},"b":[{"a":"кот","z":1}]}"#
    );
  }

  // The same request signed by the Node example of the generator's
  // docs/api.md (crypto.createHmac over JSON.stringify(sortDeep(...))).
  #[test]
  fn a_post_signature_matches_the_generators_node_example() {
    let mut body = Map::new();
    body.insert("countries".into(), serde_json::json!(["us"]));
    body.insert("name".into(), serde_json::json!("рег"));
    body.insert("ttl_minutes".into(), serde_json::json!(90));
    let signed = sign_request(
      "s3cret",
      &Method::POST,
      "/api/v1/proxies",
      &[],
      Some(body),
      1_763_200_000_000,
    );
    assert_eq!(
      signed.signature,
      "92fbcda65cbd32a755b2806e2ce43a62ffc2cfe2b1882f5d1cef620ab8ac14f0"
    );
    assert_eq!(
      signed.body["_timestamp"],
      serde_json::json!(1_763_200_000_000u64)
    );
    assert!(signed.query.is_empty());
  }

  #[test]
  fn a_get_signs_its_query_as_strings() {
    let signed = sign_request(
      "s3cret",
      &Method::GET,
      "/api/v1/proxies",
      &[("limit", "10".into())],
      None,
      1_763_200_000_000,
    );
    assert_eq!(signed.query["_timestamp"], "1763200000000");
    assert_eq!(signed.query["limit"], "10");
    assert!(signed.body.is_empty());
  }

  #[test]
  fn service_errors_map_to_owl_codes() {
    let api = |status, code: &str| Failure::Api {
      status,
      code: code.into(),
      message: "m".into(),
    };
    let code_of = |f: Failure| -> String {
      let v: Value = serde_json::from_str(&f.to_backend_error()).unwrap();
      v["code"].as_str().unwrap().to_string()
    };
    assert_eq!(
      code_of(api(401, "INVALID_SIGNATURE")),
      "PROXY_GENERATOR_KEY_INVALID"
    );
    assert_eq!(
      code_of(api(401, "TIMESTAMP_SKEW")),
      "PROXY_GENERATOR_CLOCK_SKEW"
    );
    assert_eq!(code_of(api(409, "PROXY_LIMIT")), "PROXY_GENERATOR_LIMIT");
    assert_eq!(
      code_of(api(409, "PROXY_POOL_EXHAUSTED")),
      "PROXY_GENERATOR_POOL_EXHAUSTED"
    );
    assert_eq!(code_of(api(429, "")), "PROXY_GENERATOR_RATE_LIMITED");
    assert_eq!(
      code_of(api(503, "PROVIDER_NOT_CONFIGURED")),
      "PROXY_GENERATOR_UNAVAILABLE"
    );
    assert_eq!(
      code_of(api(400, "COUNTRY_INVALID")),
      "PROXY_GENERATOR_REJECTED"
    );
    assert!(api(404, "PROXY_NOT_FOUND").is_not_found());
  }

  #[test]
  fn an_envelope_is_read_and_a_non_json_answer_is_a_failure() {
    assert_eq!(
      read_envelope(200, r#"{"ok":true,"result":{"a":1}}"#).unwrap()["a"],
      1
    );
    match read_envelope(502, "<html>bad gateway</html>") {
      Err(Failure::Api {
        status, message, ..
      }) => {
        assert_eq!(status, 502);
        assert_eq!(message, "HTTP 502");
      }
      other => panic!("unexpected {other:?}"),
    }
  }

  /// A stand-in for the generator that checks every signature the way it
  /// does and answers with what it verified.
  async fn stand_in() -> String {
    use axum::http::{HeaderMap, Method as AxumMethod, Uri};
    async fn answer(method: AxumMethod, uri: Uri, headers: HeaderMap, body: String) -> String {
      let query: Map<String, Value> =
        url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
          .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
          .collect();
      let body: Value = if method == AxumMethod::GET {
        serde_json::json!({})
      } else {
        serde_json::from_str(&body).unwrap()
      };
      let path = uri.path().trim_start_matches("/proxygen").to_string();
      let payload = serde_json::json!({
        "v": 2, "method": method.as_str(), "path": path, "query": query, "body": body
      });
      let sent = headers["x-signature"].to_str().unwrap();
      if headers["x-store-key"] != "pgk_test" || sign("s3cret", &payload) != sent {
        return r#"{"ok":false,"error":{"code":"INVALID_SIGNATURE","message":"Invalid signature"}}"#.into();
      }
      serde_json::json!({ "ok": true, "result": { "path": path, "query": query, "body": body } })
        .to_string()
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
      axum::serve(listener, axum::Router::new().fallback(answer))
        .await
        .unwrap();
    });
    format!("http://{address}/proxygen")
  }

  #[tokio::test]
  async fn signed_calls_pass_the_generators_check() {
    let base = stand_in().await;
    let pair = Credentials {
      key: "pgk_test".into(),
      secret: "s3cret".into(),
    };
    let got = call_at(
      &base,
      &pair,
      Method::GET,
      "/api/v1/proxies",
      &[("search", "кот 1".into())],
      None,
    )
    .await
    .unwrap();
    assert_eq!(got["path"], "/api/v1/proxies");
    assert_eq!(got["query"]["search"], "кот 1");

    let mut body = Map::new();
    body.insert("note".into(), Value::from("заметка"));
    let got = call_at(
      &base,
      &pair,
      Method::POST,
      "/api/v1/proxies/bulk",
      &[],
      Some(body),
    )
    .await
    .unwrap();
    assert_eq!(got["body"]["note"], "заметка");

    let got = call_at(
      &base,
      &pair,
      Method::DELETE,
      "/api/v1/proxies/abc",
      &[],
      Some(Map::new()),
    )
    .await
    .unwrap();
    assert_eq!(got["path"], "/api/v1/proxies/abc");

    let wrong = Credentials {
      key: "pgk_test".into(),
      secret: "other".into(),
    };
    let failure = call_at(&base, &wrong, Method::GET, "/api/v1/options", &[], None)
      .await
      .unwrap_err();
    assert!(failure
      .to_backend_error()
      .contains("PROXY_GENERATOR_KEY_INVALID"));
  }

  #[test]
  fn the_expiry_is_read_as_unix_seconds() {
    let proxy: GeneratedProxy = serde_json::from_value(serde_json::json!({
      "id": "u", "host": "gw.dataimpulse.com", "port": 15342, "username": "l", "password": "p",
      "expires_at": "2026-11-16T09:00:00.000Z"
    }))
    .unwrap();
    assert_eq!(proxy.expires_at_secs(), Some(1_794_819_600));
  }
}
