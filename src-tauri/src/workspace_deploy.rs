//! Install an Owl server on a VPS from the Server page: the app logs in over
//! SSH, uploads the server's sources it carries (`build_owl_server.rs`), runs
//! `workspace_deploy.sh` as root, which installs Docker and starts the server
//! behind Caddy (or the nginx already on the machine), and then joins it as
//! the owner. Running it again on the same machine updates the server and
//! keeps its data.

use russh::client;
use russh::keys::PrivateKeyWithHashAlg;
use serde::Deserialize;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::workspace::WorkspaceInfo;

const BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/owl-server.tar"));
const SCRIPT: &str = include_str!("workspace_deploy.sh");

const DEFAULT_SSH_PORT: u16 = 22;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Installing Docker and building the image on a small VPS takes minutes.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(25 * 60);
/// Caddy asks for the certificate when the server starts; it usually comes
/// within a minute.
const HTTPS_TIMEOUT: Duration = Duration::from_secs(180);
const PLAIN_PORT: u16 = 12342;
/// Lines of the script's output kept for the log when it fails.
const KEPT_LINES: usize = 40;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployRequest {
  host: String,
  port: Option<u16>,
  user: String,
  password: Option<String>,
  /// A private key file, tried before the password.
  key_path: Option<String>,
  key_passphrase: Option<String>,
  /// The server's domain name. Without one the server is named after its
  /// address on sslip.io, which resolves `1-2-3-4.sslip.io` to 1.2.3.4.
  domain: Option<String>,
  server_name: String,
  member_name: String,
  /// Debug builds only: serve plain HTTP on port 12342, for a test machine
  /// no certificate authority can reach.
  #[serde(default)]
  plain_http: bool,
}

#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum Step {
  Connect,
  Docker,
  Server,
  Https,
  Owner,
}

fn progress(step: Step) {
  let _ = crate::events::emit(
    "workspace-deploy-progress",
    serde_json::json!({ "step": step }),
  );
}

fn error(code: &str) -> String {
  crate::backend_error(code)
}

fn error_with(code: &str, params: serde_json::Value) -> String {
  serde_json::json!({ "code": code, "params": params }).to_string()
}

struct Handler;

impl client::Handler for Handler {
  type Error = russh::Error;

  /// A fresh VPS comes as an address and a password, with no host key to
  /// compare against; the key is logged so it can be checked afterwards.
  async fn check_server_key(
    &mut self,
    key: &russh::keys::PublicKeyOrCertificate,
  ) -> Result<bool, Self::Error> {
    if let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = key {
      log::info!(
        "Owl server install: host key {}",
        key.fingerprint(russh::keys::HashAlg::Sha256)
      );
    }
    Ok(true)
  }
}

type Session = client::Handle<Handler>;

/// What a command printed and how it ended.
struct Ran {
  status: Option<u32>,
  lines: Vec<String>,
}

impl Ran {
  fn ok(&self) -> bool {
    self.status == Some(0)
  }

  fn last_line(&self) -> String {
    self
      .lines
      .iter()
      .rev()
      .find(|l| !l.trim().is_empty())
      .cloned()
      .unwrap_or_default()
  }
}

/// Run `command`, feeding it `input`, and hand every output line to `on_line`
/// as it arrives.
async fn run(
  session: &Session,
  command: &str,
  input: Option<&[u8]>,
  mut on_line: impl FnMut(&str),
) -> Result<Ran, String> {
  let mut channel = session
    .channel_open_session()
    .await
    .map_err(|e| e.to_string())?;
  channel
    .exec(true, command)
    .await
    .map_err(|e| e.to_string())?;
  if let Some(input) = input {
    channel.data(input).await.map_err(|e| e.to_string())?;
  }
  channel.eof().await.map_err(|e| e.to_string())?;

  let mut status = None;
  let mut lines = Vec::new();
  let mut pending = Vec::new();
  let mut take = |bytes: &[u8], lines: &mut Vec<String>| {
    pending.extend_from_slice(bytes);
    while let Some(end) = pending.iter().position(|&b| b == b'\n') {
      let line: Vec<u8> = pending.drain(..=end).collect();
      let line = String::from_utf8_lossy(&line).trim_end().to_string();
      on_line(&line);
      lines.push(line);
      if lines.len() > KEPT_LINES {
        lines.remove(0);
      }
    }
  };
  while let Some(message) = channel.wait().await {
    match message {
      russh::ChannelMsg::Data { data } => take(&data, &mut lines),
      russh::ChannelMsg::ExtendedData { data, .. } => take(&data, &mut lines),
      russh::ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
      _ => {}
    }
  }
  take(b"\n", &mut lines);
  Ok(Ran { status, lines })
}

/// Single quotes for a shell word.
fn quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', r"'\''"))
}

/// A server name as it goes into the server's `.env`: one line, no quotes.
fn env_value(raw: &str) -> String {
  raw
    .chars()
    .filter(|c| !c.is_control() && !matches!(c, '"' | '\'' | '`' | '$' | '\\'))
    .collect::<String>()
    .trim()
    .chars()
    .take(64)
    .collect()
}

fn is_public(ip: IpAddr) -> bool {
  match ip {
    IpAddr::V4(v4) => {
      !(v4.is_private()
        || v4.is_loopback()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
    }
    IpAddr::V6(v6) => {
      !(v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00)
    }
  }
}

/// The name Caddy gets a certificate for when no domain is given.
fn sslip_name(ip: IpAddr) -> String {
  match ip {
    IpAddr::V4(v4) => format!("{}.sslip.io", v4.to_string().replace('.', "-")),
    IpAddr::V6(v6) => format!("{}.sslip.io", v6.to_string().replace(':', "-")),
  }
}

fn clean_domain(raw: &str) -> Option<String> {
  let domain = raw
    .trim()
    .trim_start_matches("https://")
    .trim_start_matches("http://")
    .trim_end_matches('/')
    .to_ascii_lowercase();
  let valid = domain.contains('.')
    && domain
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
  valid.then_some(domain)
}

async fn resolve(host: &str, port: u16) -> Option<IpAddr> {
  let addresses: Vec<_> = tokio::net::lookup_host((host, port)).await.ok()?.collect();
  addresses
    .iter()
    .find(|a| a.is_ipv4())
    .or(addresses.first())
    .map(|a| a.ip())
}

async fn connect(request: &DeployRequest, host: &str, port: u16) -> Result<Session, String> {
  let unreachable = || {
    error_with(
      "WORKSPACE_DEPLOY_UNREACHABLE",
      serde_json::json!({ "host": host }),
    )
  };
  let config = Arc::new(client::Config {
    keepalive_interval: Some(Duration::from_secs(30)),
    keepalive_max: 6,
    ..Default::default()
  });
  let mut session = tokio::time::timeout(
    CONNECT_TIMEOUT,
    client::connect(config, (host, port), Handler),
  )
  .await
  .map_err(|_| unreachable())?
  .map_err(|e| {
    log::warn!("Owl server install: SSH connection to {host}:{port} failed: {e}");
    unreachable()
  })?;

  if let Some(path) = request.key_path.as_deref().filter(|p| !p.trim().is_empty()) {
    let path = shellexpand_home(path.trim());
    let key = russh::keys::load_secret_key(
      &path,
      request.key_passphrase.as_deref().filter(|p| !p.is_empty()),
    )
    .map_err(|e| {
      log::warn!("Owl server install: cannot read the key {path}: {e}");
      error("WORKSPACE_DEPLOY_KEY_UNREADABLE")
    })?;
    let hash = session
      .best_supported_rsa_hash()
      .await
      .ok()
      .flatten()
      .flatten();
    let accepted = session
      .authenticate_publickey(
        request.user.as_str(),
        PrivateKeyWithHashAlg::new(Arc::new(key), hash),
      )
      .await
      .map_err(|e| e.to_string())?
      .success();
    if accepted {
      return Ok(session);
    }
  }
  if let Some(password) = request.password.as_deref().filter(|p| !p.is_empty()) {
    if crate::ssh_upstream::authenticate(&mut session, &request.user, password)
      .await
      .is_ok()
    {
      return Ok(session);
    }
  }
  Err(error("WORKSPACE_DEPLOY_LOGIN_REJECTED"))
}

fn shellexpand_home(path: &str) -> String {
  match path.strip_prefix("~/") {
    Some(rest) => directories::BaseDirs::new()
      .map(|d| d.home_dir().join(rest).to_string_lossy().into_owned())
      .unwrap_or_else(|| path.to_string()),
    None => path.to_string(),
  }
}

/// How this login runs a command as root: directly, through sudo without a
/// password, or through sudo with the login password on its input.
async fn root_prefix(
  session: &Session,
  password: Option<&str>,
) -> Result<(String, Option<String>), String> {
  let id = run(session, "id -u", None, |_| {}).await?;
  if id.lines.iter().any(|l| l.trim() == "0") {
    return Ok((String::new(), None));
  }
  if run(session, "sudo -n true", None, |_| {}).await?.ok() {
    return Ok(("sudo -n ".to_string(), None));
  }
  if let Some(password) = password.filter(|p| !p.is_empty()) {
    let check = run(
      session,
      "sudo -k -S -p '' true",
      Some(format!("{password}\n").as_bytes()),
      |_| {},
    )
    .await?;
    if check.ok() {
      return Ok((
        "sudo -k -S -p '' ".to_string(),
        Some(format!("{password}\n")),
      ));
    }
  }
  Err(error("WORKSPACE_DEPLOY_NO_ROOT"))
}

/// Wait until the server answers over HTTPS, which means Caddy has its
/// certificate.
async fn wait_for_https(server: &str) -> bool {
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .build()
    .unwrap_or_default();
  let deadline = tokio::time::Instant::now() + HTTPS_TIMEOUT;
  while tokio::time::Instant::now() < deadline {
    if let Ok(response) = client.get(format!("{server}/health")).send().await {
      if response.status().is_success() {
        return true;
      }
    }
    tokio::time::sleep(Duration::from_secs(4)).await;
  }
  false
}

async fn install(
  request: &DeployRequest,
  host: &str,
  public_url: &str,
  domain: &str,
  server_name: &str,
) -> Result<String, String> {
  let port = request.port.unwrap_or(DEFAULT_SSH_PORT);
  progress(Step::Connect);
  let session = connect(request, host, port).await?;
  let (sudo, sudo_input) = root_prefix(&session, request.password.as_deref()).await?;

  let failed = |step: &str, detail: String| {
    error_with(
      "WORKSPACE_DEPLOY_FAILED",
      serde_json::json!({ "step": step, "detail": detail }),
    )
  };
  let token = uuid::Uuid::new_v4().simple().to_string();
  let bundle = format!("/tmp/owl-server-{token}.tar");
  let script = format!("/tmp/owl-deploy-{token}.sh");
  for (path, bytes) in [(&bundle, BUNDLE), (&script, SCRIPT.as_bytes())] {
    let upload = run(
      &session,
      &format!("umask 077 && cat > {}", quote(path)),
      Some(bytes),
      |_| {},
    )
    .await?;
    if !upload.ok() {
      return Err(failed("upload", upload.last_line()));
    }
  }

  let command = format!(
    "{sudo}bash {} {} {} {} {}; status=$?; rm -f {}; exit $status",
    quote(&script),
    quote(&bundle),
    quote(public_url),
    quote(domain),
    quote(server_name),
    quote(&script),
  );
  let mut link = None;
  let mut has_owner = false;
  let mut unsupported = None;
  let mut ports_taken_by = None;
  let mut no_certificate = false;
  let ran = tokio::time::timeout(
    INSTALL_TIMEOUT,
    run(
      &session,
      &command,
      sudo_input.as_deref().map(str::as_bytes),
      |line| {
        if let Some(step) = line.strip_prefix("OWL_STEP ") {
          match step.trim() {
            "docker" => progress(Step::Docker),
            "server" => progress(Step::Server),
            "https" => progress(Step::Https),
            _ => {}
          }
        } else if let Some(value) = line.strip_prefix("OWL_LINK ") {
          link = Some(value.trim().to_string());
        } else if line.trim() == "OWL_HAS_OWNER" {
          has_owner = true;
        } else if let Some(os) = line.strip_prefix("OWL_ERROR unsupported_os ") {
          unsupported = Some(os.trim().to_string());
        } else if let Some(process) = line.strip_prefix("OWL_ERROR ports_busy ") {
          ports_taken_by = Some(process.trim().to_string());
        } else if line.trim() == "OWL_ERROR certificate" {
          no_certificate = true;
        }
      },
    ),
  )
  .await
  .map_err(|_| failed("timeout", String::new()))??;
  let _ = session
    .disconnect(russh::Disconnect::ByApplication, "", "en")
    .await;

  if let Some(os) = unsupported {
    return Err(error_with(
      "WORKSPACE_DEPLOY_UNSUPPORTED_OS",
      serde_json::json!({ "os": os }),
    ));
  }
  if let Some(process) = ports_taken_by {
    return Err(error_with(
      "WORKSPACE_DEPLOY_PORTS_BUSY",
      serde_json::json!({ "process": process }),
    ));
  }
  if no_certificate {
    log::warn!(
      "Owl server install on {host}: certbot failed:\n{}",
      ran.lines.join("\n")
    );
    return Err(error_with(
      "WORKSPACE_DEPLOY_HTTPS_FAILED",
      serde_json::json!({ "domain": domain }),
    ));
  }
  if !ran.ok() {
    log::warn!(
      "Owl server install on {host} failed with {:?}:\n{}",
      ran.status,
      ran.lines.join("\n")
    );
    return Err(failed("install", ran.last_line()));
  }
  if has_owner {
    return Err(error_with(
      "WORKSPACE_DEPLOY_HAS_OWNER",
      serde_json::json!({ "server": public_url }),
    ));
  }
  link.ok_or_else(|| failed("owner", ran.last_line()))
}

/// Install or update an Owl server on a VPS and join it as its owner.
#[tauri::command]
pub async fn deploy_workspace_server(
  app_handle: tauri::AppHandle,
  request: DeployRequest,
) -> Result<WorkspaceInfo, String> {
  let host = request.host.trim().trim_matches(['[', ']']).to_string();
  if host.is_empty() || request.user.trim().is_empty() {
    return Err(error_with(
      "WORKSPACE_DEPLOY_UNREACHABLE",
      serde_json::json!({ "host": host }),
    ));
  }
  if request.member_name.trim().is_empty() {
    return Err(error("NAME_CANNOT_BE_EMPTY"));
  }
  if crate::workspace::current().is_some() {
    return Err(error("WORKSPACE_ALREADY_JOINED"));
  }
  if crate::cloud_auth::CLOUD_AUTH.is_logged_in().await {
    return Err(error("SELF_HOSTED_REQUIRES_LOGOUT"));
  }
  let server_name = match env_value(&request.server_name) {
    name if name.is_empty() => "Owl".to_string(),
    name => name,
  };

  let plain_http = request.plain_http && cfg!(debug_assertions);
  let (public_url, domain) = if plain_http {
    (format!("http://{host}:{PLAIN_PORT}"), String::new())
  } else {
    let domain = match request.domain.as_deref().filter(|d| !d.trim().is_empty()) {
      Some(raw) => clean_domain(raw).ok_or_else(|| error("WORKSPACE_DEPLOY_DOMAIN_INVALID"))?,
      None => {
        let ip = resolve(&host, request.port.unwrap_or(DEFAULT_SSH_PORT))
          .await
          .ok_or_else(|| {
            error_with(
              "WORKSPACE_DEPLOY_UNREACHABLE",
              serde_json::json!({ "host": host }),
            )
          })?;
        if !is_public(ip) {
          return Err(error_with(
            "WORKSPACE_DEPLOY_PRIVATE_ADDRESS",
            serde_json::json!({ "host": host }),
          ));
        }
        sslip_name(ip)
      }
    };
    (format!("https://{domain}"), domain)
  };

  log::info!("Installing an Owl server on {host} as {public_url}");
  let link = install(&request, &host, &public_url, &domain, &server_name).await?;
  if !plain_http {
    progress(Step::Https);
    if !wait_for_https(&public_url).await {
      return Err(error_with(
        "WORKSPACE_DEPLOY_HTTPS_FAILED",
        serde_json::json!({ "domain": domain }),
      ));
    }
  }
  progress(Step::Owner);
  crate::workspace::join_workspace(app_handle, Some(link), None, None, request.member_name).await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_bundle_carries_what_the_compose_file_builds_from() {
    let mut archive = tar::Archive::new(BUNDLE);
    let names: Vec<String> = archive
      .entries()
      .unwrap()
      .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
      .collect();
    for needed in [
      ".dockerignore",
      "donut-sync/Dockerfile",
      "donut-sync/docker-compose.yml",
      "donut-sync/package.json",
      "donut-sync/src/main.ts",
      "donut-sync/src/cli.ts",
    ] {
      assert!(names.iter().any(|n| n == needed), "{needed} missing");
    }
    assert!(!names.iter().any(|n| n.ends_with(".spec.ts")));
  }

  #[test]
  fn a_server_without_a_domain_is_named_after_its_address() {
    assert_eq!(
      sslip_name("203.0.113.7".parse().unwrap()),
      "203-0-113-7.sslip.io"
    );
    assert!(is_public("203.0.113.7".parse().unwrap()));
    for private in [
      "192.168.1.10",
      "10.0.0.2",
      "127.0.0.1",
      "100.64.1.1",
      "fd00::1",
    ] {
      assert!(!is_public(private.parse().unwrap()), "{private}");
    }
  }

  #[test]
  fn a_domain_is_taken_with_or_without_its_scheme() {
    assert_eq!(
      clean_domain(" https://Owl.Example.com/ ").as_deref(),
      Some("owl.example.com")
    );
    assert_eq!(clean_domain("localhost"), None);
    assert_eq!(clean_domain("owl.example.com; rm -rf /"), None);
  }

  #[test]
  fn shell_words_and_env_values_cannot_break_out() {
    assert_eq!(quote("it's"), r"'it'\''s'");
    assert_eq!(
      env_value("  Our team\n$(reboot) 'x' "),
      "Our team(reboot) x"
    );
  }
}
