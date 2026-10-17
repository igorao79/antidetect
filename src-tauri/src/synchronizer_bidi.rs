//! The synchronizer on Camoufox, which speaks WebDriver BiDi and no DevTools.
//!
//! A leader is watched from a sandbox of each of its pages: a preload script
//! there listens for the user's mouse, keyboard and wheel events and hands each
//! one to a `script.message` channel. The page's own scripts live outside the
//! sandbox and see neither the listeners nor the channel. A preload script runs
//! in every new document, so the capture is armed again after every navigation
//! by the browser itself. Each message is rewritten into the
//! `Wayfern.inputCaptured` event the synchronizer already handles, and a
//! top-level navigation into the `Page.frameNavigated` it already replays.
//!
//! A follower is replayed with `input.performActions` and
//! `browsingContext.navigate`, and its window is placed with
//! `browser.setClientWindowState`.
//!
//! Firefox allows one BiDi session per browser, and it keeps a session after
//! its connection closes. So the session a synchronizer holds is kept here by
//! debug port, every stop ends it first (`release`), and nothing else (the
//! graceful quit, an automation client) can open one while it is held.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::synchronizer::{CapturedEvent, WindowRect};

const SANDBOX: &str = "owl_sync";
const CHANNEL: &str = "owl-sync-input";
const STEP: Duration = Duration::from_secs(10);

/// Only the top frame is watched: coordinates inside a frame are the frame's
/// own, and replaying them against the page would click somewhere else.
/// Events come out in the shape `Wayfern.inputCaptured` has, including the
/// `char` events a DevTools follower needs to type.
const CAPTURE_SCRIPT: &str = r#"(channel) => {
  if (window !== window.top || window.__owlSyncInstalled) return;
  window.__owlSyncInstalled = true;
  const send = (event) => { try { channel(JSON.stringify(event)); } catch (_) {} };
  const modifiers = (e) => (e.altKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.metaKey ? 4 : 0) | (e.shiftKey ? 8 : 0);
  const BUTTONS = ["left", "middle", "right"];
  const mouse = (type) => (e) => {
    if (!e.isTrusted) return;
    send({ type, x: e.clientX, y: e.clientY, button: BUTTONS[e.button] || "left", clickCount: e.detail || 1, modifiers: modifiers(e) });
  };
  const key = (type) => (e) => {
    if (!e.isTrusted || e.isComposing) return;
    send({ type, key: e.key, code: e.code, windowsVirtualKeyCode: e.keyCode, modifiers: modifiers(e) });
    if (type !== "keydown" || e.ctrlKey || e.metaKey) return;
    const text = e.key === "Enter" ? "\r" : e.key.length === 1 ? e.key : "";
    if (text) send({ type: "char", text, modifiers: modifiers(e) });
  };
  const LINE = 16;
  const scale = (e) => (e.deltaMode === 1 ? LINE : e.deltaMode === 2 ? window.innerHeight : 1);
  const options = { capture: true, passive: true };
  window.addEventListener("mousedown", mouse("mousedown"), options);
  window.addEventListener("mouseup", mouse("mouseup"), options);
  window.addEventListener("keydown", key("keydown"), options);
  window.addEventListener("keyup", key("keyup"), options);
  window.addEventListener("wheel", (e) => {
    if (!e.isTrusted) return;
    const k = scale(e);
    send({ type: "wheel", x: e.clientX, y: e.clientY, deltaX: -e.deltaX * k, deltaY: -e.deltaY * k });
  }, options);
}"#;

/// Engines whose profiles can take part in a synchronizer session.
pub fn is_supported(browser: &str) -> bool {
  matches!(browser, "wayfern" | "camoufox")
}

/// Whether a profile on this engine is driven over WebDriver BiDi.
pub fn speaks_bidi(browser: &str) -> bool {
  browser == "camoufox"
}

fn channel_argument() -> Value {
  json!({ "type": "channel", "value": { "channel": CHANNEL, "ownership": "none" } })
}

#[derive(Debug)]
pub enum BidiError {
  /// The connection is gone: the browser closed or the session was released.
  Lost,
  TimedOut(String),
  /// The browser answered with a WebDriver error.
  Refused {
    error: String,
    message: String,
  },
}

impl std::fmt::Display for BidiError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Lost => write!(f, "the WebDriver BiDi connection is gone"),
      Self::TimedOut(method) => write!(f, "{method} timed out"),
      Self::Refused { error, message } => write!(f, "{error}: {message}"),
    }
  }
}

enum Outgoing {
  Text(String),
  Close,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, BidiError>>>>>;

/// The one BiDi session the synchronizer holds on one browser.
#[derive(Clone)]
pub struct Bidi {
  port: u16,
  /// Tells this connection apart from a later one on the same port.
  generation: u64,
  outgoing: mpsc::UnboundedSender<Outgoing>,
  pending: Pending,
  next_id: Arc<AtomicU64>,
}

fn held() -> &'static Mutex<HashMap<u16, Bidi>> {
  static HELD: OnceLock<Mutex<HashMap<u16, Bidi>>> = OnceLock::new();
  HELD.get_or_init(|| Mutex::new(HashMap::new()))
}

static GENERATION: AtomicU64 = AtomicU64::new(1);

fn held_on(port: u16) -> Option<Bidi> {
  held().lock().ok()?.get(&port).cloned()
}

impl Bidi {
  /// Connect to the browser on `port` and open its session. The endpoint is
  /// retried for a while because the browser may still be starting.
  async fn open(port: u16) -> Result<(Self, mpsc::UnboundedReceiver<Value>), String> {
    let endpoint = format!("ws://127.0.0.1:{port}/session");
    let mut socket = None;
    for attempt in 0..15 {
      if attempt > 0 {
        tokio::time::sleep(Duration::from_secs(1)).await;
      }
      if let Ok(Ok((ws, _))) =
        tokio::time::timeout(STEP, tokio_tungstenite::connect_async(&endpoint)).await
      {
        socket = Some(ws);
        break;
      }
    }
    let socket = socket.ok_or_else(|| format!("No WebDriver BiDi endpoint on port {port}"))?;
    let (mut sink, mut stream) = socket.split();

    let (outgoing, mut outgoing_rx) = mpsc::unbounded_channel::<Outgoing>();
    let (events_tx, events_rx) = mpsc::unbounded_channel::<Value>();
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);

    tokio::spawn(async move {
      while let Some(message) = outgoing_rx.recv().await {
        match message {
          Outgoing::Text(text) => {
            if sink.send(Message::text(text)).await.is_err() {
              break;
            }
          }
          Outgoing::Close => {
            let _ = sink.close().await;
            break;
          }
        }
      }
    });

    let reader_pending = pending.clone();
    tokio::spawn(async move {
      while let Some(Ok(message)) = stream.next().await {
        let Message::Text(text) = message else {
          continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
          continue;
        };
        if value["type"] == "event" {
          let _ = events_tx.send(value);
          continue;
        }
        let Some(id) = value["id"].as_u64() else {
          continue;
        };
        let Some(reply) = reader_pending.lock().ok().and_then(|mut p| p.remove(&id)) else {
          continue;
        };
        let result = if value["type"] == "success" {
          Ok(value["result"].clone())
        } else {
          Err(BidiError::Refused {
            error: value["error"]
              .as_str()
              .unwrap_or("unknown error")
              .to_string(),
            message: value["message"].as_str().unwrap_or("").to_string(),
          })
        };
        let _ = reply.send(result);
      }
      // Waiting callers get `Lost` as their senders drop.
      if let Ok(mut pending) = reader_pending.lock() {
        pending.clear();
      }
      if let Ok(mut held) = held().lock() {
        if held.get(&port).is_some_and(|b| b.generation == generation) {
          held.remove(&port);
        }
      }
    });

    let bidi = Self {
      port,
      generation,
      outgoing,
      pending,
      next_id: Arc::new(AtomicU64::new(1)),
    };
    if let Err(e) = bidi
      .call("session.new", json!({ "capabilities": {} }))
      .await
    {
      bidi.close();
      return Err(match e {
        BidiError::Refused { error, .. } if error == "session not created" => format!(
          "Another automation client holds the browser on port {port}; close it and try again"
        ),
        other => format!("Could not open a WebDriver BiDi session: {other}"),
      });
    }
    if let Ok(mut held) = held().lock() {
      held.insert(port, bidi.clone());
    }
    Ok((bidi, events_rx))
  }

  pub async fn call(&self, method: &str, params: Value) -> Result<Value, BidiError> {
    self.call_within(method, params, STEP).await
  }

  async fn call_within(
    &self,
    method: &str,
    params: Value,
    timeout: Duration,
  ) -> Result<Value, BidiError> {
    let id = self.next_id.fetch_add(1, Ordering::Relaxed);
    let (reply, answer) = oneshot::channel();
    self
      .pending
      .lock()
      .map_err(|_| BidiError::Lost)?
      .insert(id, reply);
    let command = json!({ "id": id, "method": method, "params": params }).to_string();
    if self.outgoing.send(Outgoing::Text(command)).is_err() {
      return Err(BidiError::Lost);
    }
    match tokio::time::timeout(timeout, answer).await {
      Ok(Ok(result)) => result,
      Ok(Err(_)) => Err(BidiError::Lost),
      Err(_) => {
        if let Ok(mut pending) = self.pending.lock() {
          pending.remove(&id);
        }
        Err(BidiError::TimedOut(method.to_string()))
      }
    }
  }

  fn close(&self) {
    let _ = self.outgoing.send(Outgoing::Close);
  }

  /// End the session, so the browser takes a new one, and close the
  /// connection.
  async fn end(&self) {
    if let Ok(mut held) = held().lock() {
      if held
        .get(&self.port)
        .is_some_and(|b| b.generation == self.generation)
      {
        held.remove(&self.port);
      }
    }
    let _ = self
      .call_within("session.end", json!({}), Duration::from_secs(3))
      .await;
    self.close();
  }

  async fn top_level_contexts(&self) -> Result<Vec<String>, BidiError> {
    let tree = self
      .call("browsingContext.getTree", json!({ "maxDepth": 0 }))
      .await?;
    Ok(
      tree["contexts"]
        .as_array()
        .map(|contexts| {
          contexts
            .iter()
            .filter_map(|c| c["context"].as_str().map(str::to_string))
            .collect()
        })
        .unwrap_or_default(),
    )
  }

  /// The tab a person is looking at: a restored session can reopen several,
  /// and the first one listed is not necessarily the one in front.
  async fn visible_context(&self) -> Result<String, BidiError> {
    let contexts = self.top_level_contexts().await?;
    for context in &contexts {
      let state = self
        .call(
          "script.evaluate",
          json!({
            "expression": "document.visibilityState",
            "target": { "context": context, "sandbox": SANDBOX },
            "awaitPromise": false,
          }),
        )
        .await;
      if let Ok(state) = state {
        if state["result"]["value"] == "visible" {
          return Ok(context.clone());
        }
      }
    }
    contexts.into_iter().next().ok_or(BidiError::Refused {
      error: "no such frame".to_string(),
      message: "the browser has no tab open".to_string(),
    })
  }

  /// The browser's front window and its size.
  async fn window(&self) -> Result<(String, i64, i64), BidiError> {
    let answer = self.call("browser.getClientWindows", json!({})).await?;
    let windows = answer["clientWindows"]
      .as_array()
      .cloned()
      .unwrap_or_default();
    let window = windows
      .iter()
      .find(|w| w["active"] == true)
      .or_else(|| windows.first())
      .ok_or(BidiError::Refused {
        error: "no such window".to_string(),
        message: "the browser has no window".to_string(),
      })?;
    Ok((
      window["clientWindow"]
        .as_str()
        .unwrap_or_default()
        .to_string(),
      window["width"].as_i64().unwrap_or(0),
      window["height"].as_i64().unwrap_or(0),
    ))
  }

  async fn set_window(
    &self,
    position: Option<(i32, i32)>,
    width: u32,
    height: u32,
  ) -> Result<(), BidiError> {
    let (window, _, _) = self.window().await?;
    let mut params = json!({
      "clientWindow": window,
      "state": "normal",
      "width": width,
      "height": height,
    });
    if let Some((x, y)) = position {
      params["x"] = json!(x);
      params["y"] = json!(y);
    }
    self
      .call("browser.setClientWindowState", params)
      .await
      .map(|_| ())
  }
}

/// End the session the synchronizer holds on the browser at `port`, if any.
/// Every stop of a Camoufox calls this first, so its graceful quit can open a
/// session of its own.
pub async fn release(port: u16) {
  if let Some(bidi) = held_on(port) {
    log::info!("Synchronizer: ending the WebDriver BiDi session on port {port}");
    bidi.end().await;
  }
}

/// A Camoufox leader whose input is being watched.
pub struct Leader {
  bidi: Bidi,
  events: mpsc::UnboundedReceiver<Value>,
  top_level: HashSet<String>,
}

impl Leader {
  pub async fn attach(port: u16) -> Result<Self, String> {
    let (bidi, events) = Bidi::open(port).await?;
    match Self::arm(&bidi).await {
      Ok(top_level) => Ok(Self {
        bidi,
        events,
        top_level,
      }),
      Err(e) => {
        bidi.end().await;
        Err(format!("Could not watch the leader: {e}"))
      }
    }
  }

  async fn arm(bidi: &Bidi) -> Result<HashSet<String>, BidiError> {
    bidi
      .call(
        "session.subscribe",
        json!({
          "events": [
            "script.message",
            "browsingContext.contextCreated",
            "browsingContext.contextDestroyed",
            "browsingContext.navigationStarted",
          ]
        }),
      )
      .await?;
    // The preload script first, so a document that appears while the open
    // ones are being armed is armed by the browser.
    bidi
      .call(
        "script.addPreloadScript",
        json!({
          "functionDeclaration": CAPTURE_SCRIPT,
          "arguments": [channel_argument()],
          "sandbox": SANDBOX,
        }),
      )
      .await?;
    let contexts = bidi.top_level_contexts().await?;
    for context in &contexts {
      let armed = bidi
        .call(
          "script.callFunction",
          json!({
            "functionDeclaration": CAPTURE_SCRIPT,
            "arguments": [channel_argument()],
            "target": { "context": context, "sandbox": SANDBOX },
            "awaitPromise": false,
          }),
        )
        .await;
      // A tab still loading is armed by the preload script instead.
      if let Err(e) = armed {
        log::debug!("Synchronizer: could not arm an open tab of the leader: {e}");
      }
    }
    Ok(contexts.into_iter().collect())
  }

  /// The leader's window size, for followers to match.
  pub async fn window_size(&self) -> Option<(i64, i64)> {
    let (_, width, height) = self.bidi.window().await.ok()?;
    (width > 0 && height > 0).then_some((width, height))
  }

  /// The next event the synchronizer handles, or None once the leader has
  /// closed.
  pub async fn next(&mut self) -> Option<Value> {
    loop {
      let event = self.events.recv().await?;
      if let Some(translated) = self.translate(&event) {
        return Some(translated);
      }
    }
  }

  fn translate(&mut self, event: &Value) -> Option<Value> {
    let params = &event["params"];
    match event["method"].as_str()? {
      "script.message" => {
        if params["channel"] != CHANNEL
          || !self
            .top_level
            .contains(params["source"]["context"].as_str()?)
        {
          return None;
        }
        let payload: Value = serde_json::from_str(params["data"]["value"].as_str()?).ok()?;
        Some(json!({ "method": "Wayfern.inputCaptured", "params": payload }))
      }
      "browsingContext.navigationStarted" => {
        if !self.top_level.contains(params["context"].as_str()?) {
          return None;
        }
        Some(json!({
          "method": "Page.frameNavigated",
          "params": { "frame": { "url": params["url"] } },
        }))
      }
      "browsingContext.contextCreated" => {
        if params["parent"].is_null() {
          self
            .top_level
            .insert(params["context"].as_str()?.to_string());
        }
        None
      }
      "browsingContext.contextDestroyed" => {
        self.top_level.remove(params["context"].as_str()?);
        None
      }
      _ => None,
    }
  }
}

/// A Camoufox follower the leader's input is replayed in.
pub struct Follower {
  bidi: Bidi,
  context: String,
}

impl Follower {
  pub async fn attach(port: u16) -> Result<Self, String> {
    let (bidi, _events) = Bidi::open(port).await?;
    match bidi.visible_context().await {
      Ok(context) => Ok(Self { bidi, context }),
      Err(e) => {
        bidi.end().await;
        Err(format!("Could not find the follower's tab: {e}"))
      }
    }
  }

  /// Replay events until the session drops the channel (false) or the
  /// follower's connection is lost (true).
  pub async fn replay_all(mut self, mut events: mpsc::UnboundedReceiver<CapturedEvent>) -> bool {
    while let Some(event) = events.recv().await {
      if let Err(BidiError::Lost) = self.replay(&event).await {
        return true;
      }
    }
    false
  }

  async fn replay(&mut self, event: &CapturedEvent) -> Result<(), BidiError> {
    let Some((method, params)) = command_for(event) else {
      return Ok(());
    };
    let mut result = self.send(method, params.clone()).await;
    // The tab it was driving is gone: drive the one in front now.
    if matches!(&result, Err(BidiError::Refused { error, .. }) if error == "no such frame") {
      self.context = self.bidi.visible_context().await?;
      result = self.send(method, params).await;
    }
    match result {
      Err(BidiError::Lost) => Err(BidiError::Lost),
      Err(e) => {
        log::debug!(
          "Synchronizer: follower did not take {}: {e}",
          event.event_type
        );
        Ok(())
      }
      Ok(_) => Ok(()),
    }
  }

  async fn send(&self, method: &str, mut params: Value) -> Result<Value, BidiError> {
    params["context"] = json!(self.context);
    self.bidi.call(method, params).await
  }
}

/// Set the window of a follower whose session the synchronizer holds.
pub async fn place_window(port: u16, rect: WindowRect) -> Result<(), String> {
  let bidi = held_on(port).ok_or("The follower is not part of a session")?;
  bidi
    .set_window(Some((rect.left, rect.top)), rect.width, rect.height)
    .await
    .map_err(|e| e.to_string())
}

/// Give a follower the leader's window size.
pub async fn resize_window(port: u16, width: i64, height: i64) -> Result<(), String> {
  let bidi = held_on(port).ok_or("The follower is not part of a session")?;
  bidi
    .set_window(None, width.max(1) as u32, height.max(1) as u32)
    .await
    .map_err(|e| e.to_string())
}

/// Commands a test may send through a held session (debug builds only): the
/// session is the only one the browser has, so a scenario that drives the
/// leader or reads a follower has to go through it.
#[cfg(debug_assertions)]
pub async fn call_held(port: u16, method: &str, params: Value) -> Result<Value, String> {
  const ALLOWED: [&str; 4] = [
    "browsingContext.getTree",
    "browsingContext.navigate",
    "input.performActions",
    "script.evaluate",
  ];
  if !ALLOWED.contains(&method) {
    return Err(format!("{method} is not allowed"));
  }
  let bidi = held_on(port).ok_or("The profile is not part of a session")?;
  bidi
    .call_within(method, params, Duration::from_secs(30))
    .await
    .map_err(|e| e.to_string())
}

const MOUSE: &str = "owl-sync-mouse";
const KEYBOARD: &str = "owl-sync-keyboard";
const WHEEL: &str = "owl-sync-wheel";

fn button_number(button: Option<&str>) -> u8 {
  match button {
    Some("middle") => 1,
    Some("right") => 2,
    _ => 0,
  }
}

/// The BiDi command, without its `context`, that repeats one captured event.
/// `char` is left out: a key action types its own character.
fn command_for(event: &CapturedEvent) -> Option<(&'static str, Value)> {
  let pointer = |action: &str| {
    json!({
      "actions": [{
        "type": "pointer",
        "id": MOUSE,
        "parameters": { "pointerType": "mouse" },
        "actions": [
          { "type": "pointerMove", "x": event.x.unwrap_or(0.0).round(), "y": event.y.unwrap_or(0.0).round(), "origin": "viewport", "duration": 0 },
          { "type": action, "button": button_number(event.button.as_deref()) },
        ],
      }]
    })
  };
  let key = |action: &str| -> Option<Value> {
    let value = webdriver_key(event.key.as_deref()?, event.code.as_deref().unwrap_or(""))?;
    Some(json!({
      "actions": [{ "type": "key", "id": KEYBOARD, "actions": [{ "type": action, "value": value }] }]
    }))
  };
  match event.event_type.as_str() {
    "navigate" => Some((
      "browsingContext.navigate",
      json!({ "url": event.url.as_deref()?, "wait": "none" }),
    )),
    "mousedown" => Some(("input.performActions", pointer("pointerDown"))),
    "mouseup" => Some(("input.performActions", pointer("pointerUp"))),
    "keydown" => Some(("input.performActions", key("keyDown")?)),
    "keyup" => Some(("input.performActions", key("keyUp")?)),
    "wheel" => Some((
      "input.performActions",
      json!({
        "actions": [{
          "type": "wheel",
          "id": WHEEL,
          "actions": [{
            "type": "scroll",
            "x": event.x.unwrap_or(0.0).round() as i64,
            "y": event.y.unwrap_or(0.0).round() as i64,
            "deltaX": (-event.delta_x.unwrap_or(0.0)).round() as i64,
            "deltaY": (-event.delta_y.unwrap_or(0.0)).round() as i64,
            "origin": "viewport",
          }],
        }]
      }),
    )),
    _ => None,
  }
}

/// The WebDriver key value for a DOM `key`: the character itself, or the
/// code point WebDriver gives a named key.
fn webdriver_key(key: &str, code: &str) -> Option<String> {
  let mut chars = key.chars();
  if let (Some(c), None) = (chars.next(), chars.next()) {
    return Some(c.to_string());
  }
  let point = match (key, code) {
    ("Shift", "ShiftRight") => '\u{E050}',
    ("Control", "ControlRight") => '\u{E051}',
    ("Alt", "AltRight") => '\u{E052}',
    ("Meta", "MetaRight") => '\u{E053}',
    ("Cancel", _) => '\u{E001}',
    ("Help", _) => '\u{E002}',
    ("Backspace", _) => '\u{E003}',
    ("Tab", _) => '\u{E004}',
    ("Clear", _) => '\u{E005}',
    ("Enter", "NumpadEnter") => '\u{E007}',
    ("Enter", _) => '\u{E006}',
    ("Shift", _) => '\u{E008}',
    ("Control", _) => '\u{E009}',
    ("Alt", _) => '\u{E00A}',
    ("Pause", _) => '\u{E00B}',
    ("Escape", _) => '\u{E00C}',
    ("PageUp", _) => '\u{E00E}',
    ("PageDown", _) => '\u{E00F}',
    ("End", _) => '\u{E010}',
    ("Home", _) => '\u{E011}',
    ("ArrowLeft", _) => '\u{E012}',
    ("ArrowUp", _) => '\u{E013}',
    ("ArrowRight", _) => '\u{E014}',
    ("ArrowDown", _) => '\u{E015}',
    ("Insert", _) => '\u{E016}',
    ("Delete", _) => '\u{E017}',
    ("Meta", _) => '\u{E03D}',
    _ => {
      let number: u32 = key.strip_prefix('F')?.parse().ok()?;
      if !(1..=12).contains(&number) {
        return None;
      }
      char::from_u32(0xE031 + number - 1)?
    }
  };
  Some(point.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn event(value: Value) -> CapturedEvent {
    serde_json::from_value(value).unwrap()
  }

  fn leader() -> Leader {
    let (outgoing, _) = mpsc::unbounded_channel();
    let (_, events) = mpsc::unbounded_channel();
    Leader {
      bidi: Bidi {
        port: 0,
        generation: 0,
        outgoing,
        pending: Arc::default(),
        next_id: Arc::new(AtomicU64::new(1)),
      },
      events,
      top_level: HashSet::from(["TOP".to_string()]),
    }
  }

  #[test]
  fn a_channel_message_from_a_top_level_tab_becomes_an_input_event() {
    let mut leader = leader();
    let message = json!({
      "type": "event",
      "method": "script.message",
      "params": {
        "channel": CHANNEL,
        "data": { "type": "string", "value": r#"{"type":"mousedown","x":120,"y":90,"button":"left","clickCount":1,"modifiers":0}"# },
        "source": { "realm": "R", "context": "TOP" },
      },
    });
    let translated = leader.translate(&message).unwrap();
    assert_eq!(translated["method"], "Wayfern.inputCaptured");
    let captured: CapturedEvent = serde_json::from_value(translated["params"].clone()).unwrap();
    assert_eq!(captured.event_type, "mousedown");
    assert_eq!((captured.x, captured.y), (Some(120.0), Some(90.0)));
  }

  #[test]
  fn other_channels_frames_and_broken_payloads_are_ignored() {
    let mut leader = leader();
    for (channel, context, value) in [
      ("someone-else", "TOP", r#"{"type":"mousedown"}"#),
      (CHANNEL, "FRAME", r#"{"type":"mousedown"}"#),
      (CHANNEL, "TOP", "not json"),
    ] {
      let message = json!({
        "type": "event",
        "method": "script.message",
        "params": { "channel": channel, "data": { "type": "string", "value": value }, "source": { "context": context } },
      });
      assert!(leader.translate(&message).is_none());
    }
  }

  #[test]
  fn a_top_level_navigation_is_replayed_and_new_tabs_are_watched() {
    let mut leader = leader();
    let started = |context: &str| json!({ "type": "event", "method": "browsingContext.navigationStarted", "params": { "context": context, "url": "https://example.com/" } });
    let translated = leader.translate(&started("TOP")).unwrap();
    assert_eq!(translated["method"], "Page.frameNavigated");
    assert_eq!(translated["params"]["frame"]["url"], "https://example.com/");
    assert!(translated["params"]["frame"].get("parentId").is_none());

    assert!(leader.translate(&started("NEW")).is_none());
    leader.translate(&json!({ "method": "browsingContext.contextCreated", "params": { "context": "FRAME", "parent": "NEW" } }));
    assert!(!leader.top_level.contains("FRAME"));
    leader.translate(&json!({ "method": "browsingContext.contextCreated", "params": { "context": "NEW", "parent": null } }));
    assert!(leader.translate(&started("NEW")).is_some());
    leader.translate(
      &json!({ "method": "browsingContext.contextDestroyed", "params": { "context": "NEW" } }),
    );
    assert!(leader.translate(&started("NEW")).is_none());
  }

  #[test]
  fn a_click_becomes_a_move_and_a_press_of_the_same_button() {
    let (method, params) = command_for(&event(
      json!({ "type": "mousedown", "x": 10.4, "y": 20.6, "button": "right" }),
    ))
    .unwrap();
    assert_eq!(method, "input.performActions");
    let actions = &params["actions"][0]["actions"];
    assert_eq!(actions[0]["type"], "pointerMove");
    assert_eq!(
      (actions[0]["x"].as_f64(), actions[0]["y"].as_f64()),
      (Some(10.0), Some(21.0))
    );
    assert_eq!(actions[1]["type"], "pointerDown");
    assert_eq!(actions[1]["button"], 2);
    let (_, params) = command_for(&event(json!({ "type": "mouseup", "x": 10, "y": 20 }))).unwrap();
    assert_eq!(params["actions"][0]["actions"][1]["type"], "pointerUp");
    assert_eq!(params["actions"][0]["actions"][1]["button"], 0);
  }

  #[test]
  fn keys_type_their_character_and_named_keys_their_code_point() {
    let value = |key: &str, code: &str| {
      let (_, params) = command_for(&event(
        json!({ "type": "keydown", "key": key, "code": code }),
      ))?;
      params["actions"][0]["actions"][0]["value"]
        .as_str()
        .map(str::to_string)
    };
    assert_eq!(value("h", "KeyH").as_deref(), Some("h"));
    assert_eq!(value("ж", "Semicolon").as_deref(), Some("ж"));
    assert_eq!(value("Enter", "Enter").as_deref(), Some("\u{E006}"));
    assert_eq!(value("Shift", "ShiftRight").as_deref(), Some("\u{E050}"));
    assert_eq!(value("F5", "F5").as_deref(), Some("\u{E035}"));
    assert_eq!(value("Dead", "Quote"), None);
    assert!(command_for(&event(json!({ "type": "char", "text": "h" }))).is_none());
  }

  #[test]
  fn a_wheel_turn_scrolls_the_way_the_leader_did() {
    let (_, params) = command_for(&event(
      json!({ "type": "wheel", "x": 5, "y": 6, "deltaX": 0, "deltaY": -400 }),
    ))
    .unwrap();
    let scroll = &params["actions"][0]["actions"][0];
    assert_eq!(scroll["type"], "scroll");
    assert_eq!(scroll["deltaY"], 400);
    assert_eq!(scroll["deltaX"], 0);
  }

  #[test]
  fn a_navigation_does_not_wait_for_the_page() {
    let (method, params) = command_for(&event(
      json!({ "type": "navigate", "url": "https://example.com/" }),
    ))
    .unwrap();
    assert_eq!(method, "browsingContext.navigate");
    assert_eq!(params["wait"], "none");
  }

  #[test]
  fn the_synchronizer_takes_wayfern_and_camoufox() {
    assert!(is_supported("wayfern") && is_supported("camoufox"));
    assert!(!is_supported("cloakbrowser"));
    assert!(speaks_bidi("camoufox") && !speaks_bidi("wayfern"));
  }
}
