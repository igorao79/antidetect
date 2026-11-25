//! Cloudflare account generation through the app (fork).
//!
//! One run: a proxy from the proxy generator, a mailbox from mailhub, a
//! Camoufox profile driven over WebDriver BiDi by `scripts/cloudflare-signup`,
//! then the account (email, password, API key and token) is kept in one JSON
//! file of the app's data directory. The pipeline itself lives in that script;
//! this module owns its settings, its process and the account list.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

const ACCOUNTS_FILE: &str = "cloudflare_accounts.json";
const SETTINGS_FILE: &str = "cloudflare_settings.json";
const SECRET_FILE: &str = "cloudflare_mailhub_secret";
const SECRET_MAGIC: &[u8; 6] = b"OWLCFH";
const PROGRESS_EVENT: &str = "cloudflare-progress";

static LOCK: Mutex<()> = Mutex::new(());
static RUNNING: AtomicBool = AtomicBool::new(false);

fn code(code: &str) -> String {
  crate::backend_error(code)
}

fn detail(code: &str, err: impl std::fmt::Display) -> String {
  crate::backend_error_with_detail(code, err)
}

fn now_secs() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or_default()
}

fn data_dir() -> PathBuf {
  crate::app_dirs::data_subdir().join("cloudflare")
}

fn ensure_dir() -> Result<PathBuf, String> {
  let dir = data_dir();
  std::fs::create_dir_all(&dir).map_err(|e| detail("CLOUDFLARE_IO_FAILED", e))?;
  Ok(dir)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
  pub mailhub_key: String,
  pub proxy_uuid: Option<String>,
  pub script_path: Option<String>,
  pub country: Option<String>,
  pub ttl_minutes: Option<u32>,
  pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
  pub mailhub_key: String,
  pub has_secret: bool,
  pub proxy_uuid: Option<String>,
  pub script_path: Option<String>,
  pub country: Option<String>,
  pub ttl_minutes: Option<u32>,
  pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
  pub id: String,
  pub email: String,
  pub password: String,
  /// queued | running | verified | token | failed
  pub status: String,
  #[serde(default)]
  pub api_key: Option<String>,
  #[serde(default)]
  pub api_token: Option<String>,
  #[serde(default)]
  pub account_id: Option<String>,
  #[serde(default)]
  pub proxy_id: Option<String>,
  #[serde(default)]
  pub error: Option<String>,
  #[serde(default)]
  pub steps: Vec<String>,
  pub created_at: u64,
  #[serde(default)]
  pub finished_at: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountFile {
  #[serde(default)]
  accounts: Vec<Account>,
}

fn read_settings() -> Result<Settings, String> {
  let path = data_dir().join(SETTINGS_FILE);
  let text = match std::fs::read_to_string(&path) {
    Ok(text) => text,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
    Err(e) => return Err(detail("CLOUDFLARE_SETTINGS_FAILED", e)),
  };
  serde_json::from_str(&text).map_err(|e| detail("CLOUDFLARE_SETTINGS_FAILED", e))
}

fn write_settings(settings: &Settings) -> Result<(), String> {
  let dir = ensure_dir()?;
  let text =
    serde_json::to_string_pretty(settings).map_err(|e| detail("CLOUDFLARE_SETTINGS_FAILED", e))?;
  std::fs::write(dir.join(SETTINGS_FILE), text).map_err(|e| detail("CLOUDFLARE_SETTINGS_FAILED", e))
}

fn read_secret() -> Result<Option<String>, String> {
  crate::vault::open(&data_dir().join(SECRET_FILE), SECRET_MAGIC)
}

fn write_secret(secret: &str) -> Result<(), String> {
  ensure_dir()?;
  crate::vault::seal(&data_dir().join(SECRET_FILE), SECRET_MAGIC, secret)
}

fn read_accounts() -> Result<Vec<Account>, String> {
  let path = data_dir().join(ACCOUNTS_FILE);
  let text = match std::fs::read_to_string(&path) {
    Ok(text) => text,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(e) => return Err(detail("CLOUDFLARE_ACCOUNTS_FAILED", e)),
  };
  serde_json::from_str::<AccountFile>(&text)
    .map(|file| file.accounts)
    .map_err(|e| detail("CLOUDFLARE_ACCOUNTS_FAILED", e))
}

fn write_accounts(accounts: &[Account]) -> Result<(), String> {
  let dir = ensure_dir()?;
  let text = serde_json::to_string_pretty(&AccountFile {
    accounts: accounts.to_vec(),
  })
  .map_err(|e| detail("CLOUDFLARE_ACCOUNTS_FAILED", e))?;
  std::fs::write(dir.join(ACCOUNTS_FILE), text).map_err(|e| detail("CLOUDFLARE_ACCOUNTS_FAILED", e))
}

fn update_account(id: &str, edit: impl FnOnce(&mut Account)) -> Result<Account, String> {
  let _guard = LOCK
    .lock()
    .map_err(|e| detail("CLOUDFLARE_ACCOUNTS_FAILED", e))?;
  let mut accounts = read_accounts()?;
  let account = accounts
    .iter_mut()
    .find(|account| account.id == id)
    .ok_or_else(|| code("CLOUDFLARE_ACCOUNT_NOT_FOUND"))?;
  edit(account);
  let updated = account.clone();
  write_accounts(&accounts)?;
  Ok(updated)
}

/// The Node binary the pipeline runs on. A GUI app does not inherit the
/// shell's PATH, so the login shell is asked first.
fn node_binary() -> Result<PathBuf, String> {
  if let Ok(path) = std::env::var("OWL_NODE") {
    let path = PathBuf::from(path);
    if path.is_file() {
      return Ok(path);
    }
  }
  for candidate in [
    "/opt/homebrew/bin/node",
    "/usr/local/bin/node",
    "/usr/bin/node",
  ] {
    let path = PathBuf::from(candidate);
    if path.is_file() {
      return Ok(path);
    }
  }
  let output = Command::new("/bin/zsh")
    .args(["-lc", "command -v node"])
    .output()
    .map_err(|e| detail("CLOUDFLARE_NODE_MISSING", e))?;
  let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
  if path.is_empty() {
    return Err(code("CLOUDFLARE_NODE_MISSING"));
  }
  Ok(PathBuf::from(path))
}

/// The pipeline script. Kept next to the repository in development; the
/// settings field overrides it for an installed build.
fn script_path(settings: &Settings) -> Result<PathBuf, String> {
  let mut candidates: Vec<PathBuf> = Vec::new();
  if let Some(path) = settings
    .script_path
    .as_deref()
    .map(str::trim)
    .filter(|path| !path.is_empty())
  {
    candidates.push(PathBuf::from(path));
  }
  if let Ok(path) = std::env::var("OWL_CF_SCRIPT") {
    candidates.push(PathBuf::from(path));
  }
  if let Ok(cwd) = std::env::current_dir() {
    candidates.push(cwd.join("scripts/cloudflare-signup/signup.mjs"));
    if let Some(parent) = cwd.parent() {
      candidates.push(parent.join("scripts/cloudflare-signup/signup.mjs"));
    }
  }
  if let Ok(home) = std::env::var("HOME") {
    candidates.push(
      PathBuf::from(home).join("Desktop/projects/antidetect/scripts/cloudflare-signup/signup.mjs"),
    );
  }
  candidates
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| code("CLOUDFLARE_SCRIPT_MISSING"))
}

fn status_from_line(line: &str) -> Option<&'static str> {
  let lower = line.to_lowercase();
  if lower.contains("mailbox:") {
    Some("mailbox")
  } else if lower.contains("proxy:") {
    Some("proxy")
  } else if lower.contains("profile:") {
    Some("profile")
  } else if lower.contains("browser:") {
    Some("browser")
  } else if lower.contains("turnstile:") {
    Some("signup")
  } else if lower.contains("submit:") {
    Some("submitted")
  } else if lower.contains("verification link") || lower.contains("email verification") {
    Some("email")
  } else if lower.contains("api key:") || lower.contains("api token") {
    Some("token")
  } else if lower.starts_with("done") || lower.contains("failed:") {
    Some("finished")
  } else {
    None
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
  #[serde(default)]
  email: Option<String>,
  #[serde(default)]
  password: Option<String>,
  #[serde(default)]
  outcome: Option<String>,
  #[serde(default)]
  api_key: Option<String>,
  #[serde(default)]
  api_token: Option<String>,
  #[serde(default)]
  account_id: Option<String>,
  #[serde(default)]
  error: Option<String>,
}

#[tauri::command]
pub fn cloudflare_settings() -> Result<SettingsView, String> {
  let settings = read_settings()?;
  let secret = read_secret()?;
  Ok(SettingsView {
    mailhub_key: settings.mailhub_key,
    has_secret: secret.map(|s| !s.is_empty()).unwrap_or(false),
    proxy_uuid: settings.proxy_uuid,
    script_path: settings.script_path,
    country: settings.country,
    ttl_minutes: settings.ttl_minutes,
    domain: settings.domain,
  })
}

#[tauri::command]
pub fn cloudflare_save_settings(
  mailhub_key: String,
  mailhub_secret: Option<String>,
  proxy_uuid: Option<String>,
  script_path: Option<String>,
  country: Option<String>,
  ttl_minutes: Option<u32>,
  domain: Option<String>,
) -> Result<SettingsView, String> {
  let mut settings = read_settings()?;
  settings.mailhub_key = mailhub_key.trim().to_string();
  settings.proxy_uuid = proxy_uuid
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty());
  settings.script_path = script_path
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty());
  settings.country = country
    .map(|v| v.trim().to_lowercase())
    .filter(|v| !v.is_empty());
  settings.ttl_minutes = ttl_minutes.filter(|v| *v > 0);
  settings.domain = domain
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty());
  write_settings(&settings)?;
  if let Some(secret) = mailhub_secret {
    let secret = secret.trim();
    if !secret.is_empty() {
      write_secret(secret)?;
    }
  }
  cloudflare_settings()
}

#[tauri::command]
pub fn cloudflare_accounts() -> Result<Vec<Account>, String> {
  read_accounts()
}

#[tauri::command]
pub fn cloudflare_delete_account(id: String) -> Result<(), String> {
  let _guard = LOCK
    .lock()
    .map_err(|e| detail("CLOUDFLARE_ACCOUNTS_FAILED", e))?;
  let mut accounts = read_accounts()?;
  accounts.retain(|account| account.id != id);
  write_accounts(&accounts)
}

/// Runs one generation. The proxy comes from the generator unless the settings
/// name a stored proxy; the script then does the browser work and writes a
/// report this command turns into an account entry.
#[tauri::command]
pub async fn cloudflare_generate(app: AppHandle) -> Result<Account, String> {
  if RUNNING.swap(true, Ordering::SeqCst) {
    return Err(code("CLOUDFLARE_RUNNING"));
  }
  let result = generate_inner(&app).await;
  RUNNING.store(false, Ordering::SeqCst);
  result
}

fn read_pipe(pipe: Option<Box<dyn std::io::Read + Send>>, progress: bool, id: &str) -> Vec<String> {
  let mut lines = Vec::new();
  if let Some(pipe) = pipe {
    for line in BufReader::new(pipe).lines().map_while(Result::ok) {
      if progress {
        if let Some(status) = status_from_line(&line) {
          let _ = crate::events::emit(
            PROGRESS_EVENT,
            serde_json::json!({ "id": id, "status": status, "line": line }),
          );
        }
      }
      lines.push(line);
    }
  }
  lines
}

fn owl_base() -> String {
  if let Ok(port) = std::env::var("ANTIDETECT_LOCAL_API_PORT") {
    if !port.trim().is_empty() && port != "0" {
      return format!("http://127.0.0.1:{}", port.trim());
    }
  }
  "http://127.0.0.1:58888".to_string()
}

async fn generate_inner(app: &AppHandle) -> Result<Account, String> {
  let settings = read_settings()?;
  if settings.mailhub_key.trim().is_empty() {
    return Err(code("CLOUDFLARE_MAILHUB_MISSING"));
  }
  let secret = read_secret()?.ok_or_else(|| code("CLOUDFLARE_MAILHUB_MISSING"))?;
  let node = node_binary()?;
  let script = script_path(&settings)?;

  let id = uuid::Uuid::new_v4().to_string();
  let account = Account {
    id: id.clone(),
    email: String::new(),
    password: String::new(),
    status: "queued".to_string(),
    api_key: None,
    api_token: None,
    account_id: None,
    proxy_id: settings.proxy_uuid.clone(),
    error: None,
    steps: Vec::new(),
    created_at: now_secs(),
    finished_at: None,
  };
  {
    let _guard = LOCK
      .lock()
      .map_err(|e| detail("CLOUDFLARE_ACCOUNTS_FAILED", e))?;
    let mut accounts = read_accounts()?;
    accounts.insert(0, account.clone());
    write_accounts(&accounts)?;
  }

  // A generated proxy becomes an ordinary stored proxy; the script assigns it.
  let mut proxy_id = settings.proxy_uuid.clone();
  if proxy_id.is_none() {
    let request = crate::proxy_generator::GenerateRequest {
      count: 1,
      countries_mode: settings.country.clone().map(|_| "only".to_string()),
      countries: settings.country.clone().into_iter().collect(),
      ttl_minutes: settings.ttl_minutes,
      name: Some(format!("cf {}", id.chars().take(6).collect::<String>())),
      note: Some("cloudflare signup".to_string()),
      protocol: Default::default(),
    };
    let generated = crate::proxy_generator::proxy_generator_generate(app.clone(), request).await?;
    proxy_id = generated.first().map(|proxy| proxy.id.clone());
    if proxy_id.is_none() {
      let message = crate::backend_error("CLOUDFLARE_PROXY_FAILED");
      update_account(&id, |account| {
        account.status = "failed".to_string();
        account.error = Some(message.clone());
        account.finished_at = Some(now_secs());
      })?;
      return Err(message);
    }
    update_account(&id, |account| account.proxy_id = proxy_id.clone())?;
  }

  let dir = ensure_dir()?;
  let report_path = dir.join(format!("report-{id}.json"));
  let proxy = proxy_id.clone().unwrap_or_default();
  let mut args: Vec<String> = vec![
    script.to_string_lossy().to_string(),
    "--proxy-uuid".to_string(),
    proxy,
    "--os".to_string(),
    "win".to_string(),
    "--key-rounds".to_string(),
    "4".to_string(),
    "--key-wait".to_string(),
    "75".to_string(),
    "--report".to_string(),
    report_path.to_string_lossy().to_string(),
  ];
  if let Some(domain) = settings.domain.clone() {
    args.push("--domain".to_string());
    args.push(domain);
  }

  update_account(&id, |account| account.status = "running".to_string())?;
  let _ = crate::events::emit(
    PROGRESS_EVENT,
    serde_json::json!({ "id": id, "status": "running", "line": "started" }),
  );

  let mut child = Command::new(&node)
    .args(&args)
    .env("MAILHUB_KEY", settings.mailhub_key.trim())
    .env("MAILHUB_SECRET", secret.trim())
    .env("OWL_BASE", owl_base())
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| detail("CLOUDFLARE_SPAWN_FAILED", e))?;

  let stdout = child.stdout.take();
  let stderr = child.stderr.take();
  let out_id = id.clone();
  let err_id = id.clone();
  let out_thread = std::thread::spawn(move || {
    read_pipe(
      stdout.map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
      true,
      &out_id,
    )
  });
  let err_thread = std::thread::spawn(move || {
    read_pipe(
      stderr.map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
      false,
      &err_id,
    )
  });
  let mut lines = out_thread.join().unwrap_or_default();
  lines.extend(err_thread.join().unwrap_or_default());
  let status = child
    .wait()
    .map_err(|e| detail("CLOUDFLARE_SPAWN_FAILED", e))?;

  let report: Option<Report> = std::fs::read_to_string(&report_path)
    .ok()
    .and_then(|text| serde_json::from_str(&text).ok());
  let tail: Vec<String> = lines.iter().rev().take(12).rev().cloned().collect();
  let report = report.ok_or_else(|| {
    let message = tail.join(" / ");
    detail("CLOUDFLARE_REPORT_MISSING", message)
  })?;

  let token_ready = report.api_token.is_some();
  let outcome = report.outcome.clone().unwrap_or_default();
  let final_status = if outcome.starts_with("created") {
    if token_ready {
      "token"
    } else {
      "verified"
    }
  } else {
    "failed"
  };
  let updated = update_account(&id, |account| {
    account.email = report.email.clone().unwrap_or_default();
    account.password = report.password.clone().unwrap_or_default();
    account.api_key = report.api_key.clone();
    account.api_token = report.api_token.clone();
    account.account_id = report.account_id.clone();
    account.status = final_status.to_string();
    account.error = report.error.clone().or_else(|| {
      if status.success() {
        None
      } else {
        Some(tail.join(" / "))
      }
    });
    account.steps = tail.clone();
    account.finished_at = Some(now_secs());
  })?;
  let _ = crate::events::emit(
    PROGRESS_EVENT,
    serde_json::json!({ "id": id, "status": final_status, "line": "finished" }),
  );
  Ok(updated)
}
