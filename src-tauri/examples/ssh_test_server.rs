//! Stand-alone SSH proxy for the launch scenarios in `scripts/scenarios/`:
//! `cargo run --example ssh_test_server -- <port> <user> <password> [prompt]`.
//! Listens on loopback only; `prompt` takes the password through
//! keyboard-interactive instead of the `password` method.

#[path = "../tests/common/ssh_server.rs"]
mod ssh_server;

use ssh_server::{PasswordVia, TestSshServer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let [port, user, password, rest @ ..] = args.as_slice() else {
    eprintln!("usage: ssh_test_server <port> <user> <password> [prompt]");
    std::process::exit(2);
  };
  let port: u16 = port.parse().map_err(std::io::Error::other)?;
  let via = if rest.first().is_some_and(|mode| mode == "prompt") {
    PasswordVia::Prompt
  } else {
    PasswordVia::Password
  };

  let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
  println!("ssh test server on 127.0.0.1:{port}");
  TestSshServer {
    user: user.clone(),
    password: password.clone(),
    via,
    sessions: Default::default(),
    log_forwards: true,
  }
  .serve(listener)
  .await
}
