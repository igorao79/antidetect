//! SSH as an upstream proxy type (`ssh://user:password@host:port`).
//!
//! The local proxy worker keeps one SSH session per upstream and carries every
//! browser connection in its own `direct-tcpip` channel, the way `ssh -D`
//! does. SSH forwards TCP only, so UDP is refused by the callers and the
//! browser falls back to proxied TCP.

use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Bytes, Incoming};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use russh::client::{self, KeyboardInteractiveAuthResponse};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use url::Url;

pub const SCHEME: &str = "ssh";
const DEFAULT_PORT: u16 = 22;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
const CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(20);
const HTTP_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(60);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// How long a failed handshake answers for the connections queued behind it,
/// so a dead server costs one timeout and not one per pending request.
const FAILURE_MEMORY: Duration = Duration::from_secs(5);
/// A server asking more questions than this is not asking for a password.
const MAX_INTERACTIVE_ROUNDS: usize = 3;

struct Handler;

impl client::Handler for Handler {
  type Error = russh::Error;

  /// Every host key is accepted. A rented SSH proxy comes as a host and a
  /// password with no key to compare against, so the hop is safe from a
  /// passive listener and not from someone who can answer in the server's
  /// place.
  async fn check_server_key(
    &mut self,
    _server_public_key: &russh::keys::PublicKeyOrCertificate,
  ) -> Result<bool, Self::Error> {
    Ok(true)
  }
}

type Session = Arc<client::Handle<Handler>>;

enum Slot {
  Live(Session),
  Failed { at: Instant, reason: String },
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Slot>>> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

fn credentials(upstream: &Url) -> (String, String) {
  let decode = |value: &str| {
    urlencoding::decode(value)
      .map(|v| v.into_owned())
      .unwrap_or_else(|_| value.to_string())
  };
  (
    decode(upstream.username()),
    decode(upstream.password().unwrap_or_default()),
  )
}

pub(crate) async fn authenticate<H: client::Handler>(
  handle: &mut client::Handle<H>,
  user: &str,
  password: &str,
) -> Result<(), String> {
  let rejected = || "SSH server rejected the login".to_string();
  let failed = |e: russh::Error| format!("SSH authentication failed: {e}");

  if handle
    .authenticate_password(user, password)
    .await
    .map_err(failed)?
    .success()
  {
    return Ok(());
  }

  // Servers set up through PAM often take the password only as the answer to
  // a keyboard-interactive prompt.
  let mut reply = handle
    .authenticate_keyboard_interactive_start(user, None)
    .await
    .map_err(failed)?;
  for _ in 0..MAX_INTERACTIVE_ROUNDS {
    match reply {
      KeyboardInteractiveAuthResponse::Success => return Ok(()),
      KeyboardInteractiveAuthResponse::Failure { .. } => return Err(rejected()),
      KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
        let answers = prompts.iter().map(|_| password.to_string()).collect();
        reply = handle
          .authenticate_keyboard_interactive_respond(answers)
          .await
          .map_err(failed)?;
      }
    }
  }
  Err(rejected())
}

async fn open_session(upstream: &Url) -> Result<client::Handle<Handler>, String> {
  let host = upstream
    .host_str()
    .ok_or("SSH proxy has no host")?
    .trim_matches(['[', ']'])
    .to_string();
  let port = upstream.port().unwrap_or(DEFAULT_PORT);
  let (user, password) = credentials(upstream);
  if user.is_empty() {
    return Err("SSH proxy needs a username".to_string());
  }

  let config = Arc::new(client::Config {
    keepalive_interval: Some(KEEPALIVE_INTERVAL),
    keepalive_max: 3,
    nodelay: true,
    ..Default::default()
  });

  tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
    let mut handle = client::connect(config, (host.as_str(), port), Handler)
      .await
      .map_err(|e| format!("SSH connection to {host}:{port} failed: {e}"))?;
    authenticate(&mut handle, &user, &password).await?;
    Ok(handle)
  })
  .await
  .map_err(|_| format!("SSH connection to {host}:{port} timed out"))?
}

/// The shared session for this upstream, opened on first use and reopened
/// after the server drops it.
async fn session(upstream: &Url) -> Result<Session, String> {
  let mut sessions = SESSIONS.lock().await;
  match sessions.get(upstream.as_str()) {
    Some(Slot::Live(session)) if !session.is_closed() => return Ok(session.clone()),
    Some(Slot::Failed { at, reason }) if at.elapsed() < FAILURE_MEMORY => {
      return Err(reason.clone())
    }
    _ => {}
  }

  // Held across the handshake on purpose: connections arriving meanwhile wait
  // for this session instead of each opening their own.
  match open_session(upstream).await {
    Ok(handle) => {
      let session = Arc::new(handle);
      sessions.insert(upstream.as_str().to_string(), Slot::Live(session.clone()));
      Ok(session)
    }
    Err(reason) => {
      log::warn!("{reason}");
      sessions.insert(
        upstream.as_str().to_string(),
        Slot::Failed {
          at: Instant::now(),
          reason: reason.clone(),
        },
      );
      Err(reason)
    }
  }
}

/// Open a TCP stream to `host:port` from the far end of the SSH session.
pub async fn connect(
  upstream: &Url,
  host: &str,
  port: u16,
) -> Result<impl AsyncRead + AsyncWrite + Unpin + Send, String> {
  let session = session(upstream).await?;
  let channel = tokio::time::timeout(
    CHANNEL_OPEN_TIMEOUT,
    session.channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0),
  )
  .await
  .map_err(|_| format!("SSH proxy timed out connecting to {host}:{port}"))?
  .map_err(|e| format!("SSH proxy could not connect to {host}:{port}: {e}"))?;
  Ok(channel.into_stream())
}

fn bad_gateway(reason: impl Into<String>) -> Response<Full<Bytes>> {
  let mut response = Response::new(Full::new(Bytes::from(reason.into())));
  *response.status_mut() = StatusCode::BAD_GATEWAY;
  response
}

async fn exchange(
  req: Request<Incoming>,
  upstream: &Url,
  host: &str,
) -> Result<Response<Full<Bytes>>, String> {
  let port = req.uri().port_u16().unwrap_or(80);
  let stream = connect(upstream, host, port).await?;
  let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
    .await
    .map_err(|e| format!("HTTP handshake over SSH failed: {e}"))?;
  tokio::spawn(connection);

  // The browser sent a proxy request (absolute URI); the origin gets the path.
  let (mut parts, body) = req.into_parts();
  let authority = parts.uri.authority().map(|a| a.as_str().to_string());
  let path = parts
    .uri
    .path_and_query()
    .map(|pq| pq.as_str())
    .unwrap_or("/");
  parts.uri = path
    .parse()
    .map_err(|e| format!("Invalid request path: {e}"))?;
  parts.headers.remove("proxy-connection");
  parts.headers.remove("proxy-authorization");
  if !parts.headers.contains_key(hyper::header::HOST) {
    if let Some(value) = authority.and_then(|a| a.parse().ok()) {
      parts.headers.insert(hyper::header::HOST, value);
    }
  }

  let response = sender
    .send_request(Request::from_parts(parts, body))
    .await
    .map_err(|e| format!("HTTP request over SSH failed: {e}"))?;
  let (mut parts, body) = response.into_parts();
  let body = body
    .collect()
    .await
    .map_err(|e| format!("HTTP response over SSH was cut short: {e}"))?
    .to_bytes();
  // The body is handed on whole, so it is no longer chunked.
  parts.headers.remove(hyper::header::TRANSFER_ENCODING);
  Ok(Response::from_parts(parts, Full::new(body)))
}

/// Serve one plain-HTTP proxy request through the SSH session. HTTPS never
/// comes here: it arrives as CONNECT and is tunnelled by [`connect`].
pub async fn handle_http(
  req: Request<Incoming>,
  upstream: &Url,
) -> Result<Response<Full<Bytes>>, Infallible> {
  let Some(host) = req.uri().host().map(str::to_string) else {
    return Ok(bad_gateway("Request has no host"));
  };
  let sent = req.body().size_hint().lower();

  let response =
    match tokio::time::timeout(HTTP_EXCHANGE_TIMEOUT, exchange(req, upstream, &host)).await {
      Ok(Ok(response)) => response,
      Ok(Err(reason)) => return Ok(bad_gateway(reason)),
      Err(_) => return Ok(bad_gateway("SSH proxy timed out")),
    };

  if let Some(tracker) = crate::traffic_stats::get_traffic_tracker() {
    let received = response.body().size_hint().lower();
    tracker.record_request(&host, sent, received);
  }
  Ok(response)
}

#[cfg(test)]
#[path = "../tests/common/ssh_server.rs"]
mod test_server;

#[cfg(test)]
mod tests {
  use super::test_server::{PasswordVia, TestSshServer};
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  struct Proxy {
    port: u16,
    sessions: Arc<AtomicUsize>,
  }

  impl Proxy {
    fn upstream(&self, password: &str) -> Url {
      Url::parse(&format!("ssh://tester:{password}@127.0.0.1:{}", self.port)).unwrap()
    }
  }

  async fn start_proxy(via: PasswordVia) -> Proxy {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let sessions = Arc::new(AtomicUsize::new(0));
    let server = TestSshServer {
      user: "tester".to_string(),
      password: "s3cret".to_string(),
      via,
      sessions: sessions.clone(),
      log_forwards: false,
    };
    tokio::spawn(server.serve(listener));
    Proxy { port, sessions }
  }

  /// A target that answers every connection with what it was sent.
  async fn start_echo() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
      while let Ok((mut socket, _)) = listener.accept().await {
        tokio::spawn(async move {
          let (mut reader, mut writer) = socket.split();
          let _ = tokio::io::copy(&mut reader, &mut writer).await;
        });
      }
    });
    port
  }

  async fn round_trip(upstream: &Url, port: u16, payload: &[u8]) -> Vec<u8> {
    let mut stream = connect(upstream, "127.0.0.1", port).await.unwrap();
    stream.write_all(payload).await.unwrap();
    let mut answer = vec![0u8; payload.len()];
    stream.read_exact(&mut answer).await.unwrap();
    answer
  }

  #[test]
  fn credentials_are_percent_decoded() {
    let upstream = Url::parse("ssh://ro%40ot:p%3Ass%2Fw%23rd@198.51.100.7:2222").unwrap();
    assert_eq!(
      credentials(&upstream),
      ("ro@ot".to_string(), "p:ss/w#rd".to_string())
    );
  }

  #[tokio::test]
  async fn connections_travel_through_one_shared_session() {
    let proxy = start_proxy(PasswordVia::Password).await;
    let echo = start_echo().await;
    let upstream = proxy.upstream("s3cret");

    assert_eq!(round_trip(&upstream, echo, b"first").await, b"first");
    assert_eq!(round_trip(&upstream, echo, b"second").await, b"second");
    assert_eq!(proxy.sessions.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn a_password_asked_for_by_prompt_is_answered() {
    let proxy = start_proxy(PasswordVia::Prompt).await;
    let echo = start_echo().await;

    assert_eq!(
      round_trip(&proxy.upstream("s3cret"), echo, b"hello").await,
      b"hello"
    );
  }

  #[tokio::test]
  async fn a_wrong_password_is_reported_as_a_rejected_login() {
    for via in [PasswordVia::Password, PasswordVia::Prompt] {
      let proxy = start_proxy(via).await;
      let error = connect(&proxy.upstream("wrong"), "127.0.0.1", 80)
        .await
        .err()
        .unwrap();
      assert!(error.contains("rejected the login"), "{error}");
    }
  }

  #[tokio::test]
  async fn an_unreachable_target_fails_without_costing_the_session() {
    let proxy = start_proxy(PasswordVia::Password).await;
    let echo = start_echo().await;
    let upstream = proxy.upstream("s3cret");
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);

    let error = connect(&upstream, "127.0.0.1", closed_port)
      .await
      .err()
      .unwrap();
    assert!(error.contains("could not connect"), "{error}");
    assert_eq!(round_trip(&upstream, echo, b"still up").await, b"still up");
    assert_eq!(proxy.sessions.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn a_dropped_session_is_reopened_for_the_next_connection() {
    let echo = start_echo().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = Url::parse(&format!("ssh://tester:s3cret@127.0.0.1:{port}")).unwrap();
    let server = |sessions: &Arc<AtomicUsize>| TestSshServer {
      user: "tester".to_string(),
      password: "s3cret".to_string(),
      via: PasswordVia::Password,
      sessions: sessions.clone(),
      log_forwards: false,
    };

    let first = Arc::new(AtomicUsize::new(0));
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server(&first).serve_until(listener, async {
      let _ = stopped.await;
    }));
    assert_eq!(round_trip(&upstream, echo, b"before").await, b"before");

    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();
    let second = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    tokio::spawn(server(&second).serve(listener));

    let mut answer = None;
    for _ in 0..50 {
      if let Ok(mut stream) = connect(&upstream, "127.0.0.1", echo).await {
        stream.write_all(b"after").await.unwrap();
        let mut buffer = [0u8; 5];
        stream.read_exact(&mut buffer).await.unwrap();
        answer = Some(buffer);
        break;
      }
      tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(answer.as_ref(), Some(b"after"));
    assert_eq!(second.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn a_session_without_a_username_is_refused_before_dialling() {
    let upstream = Url::parse("ssh://198.51.100.7:22").unwrap();
    let error = open_session(&upstream).await.err().unwrap();
    assert!(error.contains("username"), "{error}");
  }

  #[tokio::test]
  async fn a_failed_handshake_answers_for_the_connections_behind_it() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let upstream = Url::parse(&format!("ssh://user:pw@127.0.0.1:{port}")).unwrap();

    let first = connect(&upstream, "example.com", 443).await.err().unwrap();
    let started = Instant::now();
    let second = connect(&upstream, "example.com", 443).await.err().unwrap();
    assert_eq!(first, second);
    assert!(started.elapsed() < Duration::from_secs(1));
  }
}
