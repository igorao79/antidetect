//! A password SSH server that forwards `direct-tcpip` channels, for exercising
//! the SSH upstream without a real host. Shared by the unit tests in
//! `src/ssh_upstream.rs` and the `ssh_test_server` example.

use russh::server::{self, Auth, ChannelOpenHandle, Msg, Response, Server as _, Session};
use russh::{Channel, ChannelOpenFailure, MethodKind, MethodSet};
use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

/// How the server takes the password.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PasswordVia {
  /// The `password` method, as OpenSSH does with `PasswordAuthentication yes`.
  Password,
  /// Only as the answer to a keyboard-interactive prompt, as PAM setups do.
  Prompt,
}

#[derive(Clone)]
pub struct TestSshServer {
  pub user: String,
  pub password: String,
  pub via: PasswordVia,
  /// Sessions opened so far, for asserting that connections share one.
  pub sessions: Arc<AtomicUsize>,
  /// Print one `forward host:port` line per channel, so a caller reading
  /// stdout can tell what went through.
  pub log_forwards: bool,
}

impl TestSshServer {
  fn accepts(&self, user: &str, password: &[u8]) -> bool {
    user == self.user && password == self.password.as_bytes()
  }

  /// Serve on `listener` until `stop` resolves, then disconnect every client.
  pub async fn serve_until(
    mut self,
    listener: TcpListener,
    stop: impl std::future::Future<Output = ()> + Send + 'static,
  ) -> std::io::Result<()> {
    let key = russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
      .map_err(std::io::Error::other)?;
    let config = Arc::new(server::Config {
      keys: vec![key],
      auth_rejection_time: Duration::from_millis(10),
      auth_rejection_time_initial: Some(Duration::ZERO),
      ..Default::default()
    });
    let running = self.run_on_socket(config, &listener);
    let handle = running.handle();
    tokio::spawn(async move {
      stop.await;
      handle.shutdown("test server stopped".to_string());
    });
    running.await
  }

  /// Serve on `listener` until the task is dropped.
  pub async fn serve(self, listener: TcpListener) -> std::io::Result<()> {
    self.serve_until(listener, std::future::pending()).await
  }
}

impl server::Server for TestSshServer {
  type Handler = Self;

  fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> Self {
    self.sessions.fetch_add(1, Ordering::SeqCst);
    self.clone()
  }
}

impl server::Handler for TestSshServer {
  type Error = russh::Error;

  async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
    if self.via == PasswordVia::Prompt {
      return Ok(Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::KeyboardInteractive][..])),
        partial_success: false,
      });
    }
    Ok(if self.accepts(user, password.as_bytes()) {
      Auth::Accept
    } else {
      Auth::reject()
    })
  }

  async fn auth_keyboard_interactive<'a>(
    &'a mut self,
    user: &str,
    _submethods: &str,
    response: Option<Response<'a>>,
  ) -> Result<Auth, Self::Error> {
    if self.via != PasswordVia::Prompt {
      return Ok(Auth::reject());
    }
    let Some(mut answers) = response else {
      return Ok(Auth::Partial {
        name: Cow::Borrowed(""),
        instructions: Cow::Borrowed(""),
        prompts: Cow::Borrowed(&[(Cow::Borrowed("Password: "), false)]),
      });
    };
    Ok(match answers.next() {
      Some(answer) if self.accepts(user, &answer) => Auth::Accept,
      _ => Auth::reject(),
    })
  }

  async fn channel_open_direct_tcpip(
    &mut self,
    channel: Channel<Msg>,
    host_to_connect: &str,
    port_to_connect: u32,
    _originator_address: &str,
    _originator_port: u32,
    reply: ChannelOpenHandle,
    _session: &mut Session,
  ) -> Result<(), Self::Error> {
    let port = u16::try_from(port_to_connect).unwrap_or_default();
    if self.log_forwards {
      println!("forward {host_to_connect}:{port}");
    }
    match TcpStream::connect((host_to_connect, port)).await {
      Ok(mut target) => {
        reply.accept().await;
        tokio::spawn(async move {
          let mut stream = channel.into_stream();
          let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
        });
      }
      Err(_) => reply.reject(ChannelOpenFailure::ConnectFailed).await,
    }
    Ok(())
  }
}
