//! Input capture for a synchronizer leader that is not Wayfern.
//!
//! Wayfern reports the leader's input itself (`Wayfern.enableInputCapture`).
//! Any other Chromium is watched from an isolated world of the page instead: a
//! script there listens for the user's mouse, keyboard and wheel events and
//! hands each one to a DevTools binding. The page's own scripts live in another
//! world and see neither the listeners nor the binding, and the `Runtime`
//! domain is never enabled.
//!
//! An isolated world dies with its document, so it is set up again after every
//! top-level navigation. Each binding call is rewritten into the
//! `Wayfern.inputCaptured` event the synchronizer already replays.

use serde_json::{json, Value};

const WORLD: &str = "antidetect_sync";
const BINDING: &str = "__antidetectSyncInput";
/// Above anything the synchronizer's own command counter reaches, and inside
/// the 32-bit range DevTools accepts for a command id.
const FIRST_COMMAND_ID: u64 = 1_000_000_000;

/// Only the top frame is watched: coordinates inside a frame are the frame's
/// own, and replaying them against the page would click somewhere else.
const CAPTURE_SCRIPT: &str = r#"(() => {
  if (window !== window.top || window.__captureInstalled) return;
  window.__captureInstalled = true;
  const send = (event) => { try { __antidetectSyncInput(JSON.stringify(event)); } catch (_) {} };
  const modifiers = (e) => (e.altKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.metaKey ? 4 : 0) | (e.shiftKey ? 8 : 0);
  const BUTTONS = ["left", "middle", "right"];
  const mouse = (type) => (e) => {
    if (!e.isTrusted) return;
    send({ type, x: e.clientX, y: e.clientY, button: BUTTONS[e.button] || "left", clickCount: e.detail || 1, modifiers: modifiers(e) });
  };
  const key = (type) => (e) => {
    if (!e.isTrusted) return;
    send({ type, key: e.key, code: e.code, windowsVirtualKeyCode: e.keyCode, modifiers: modifiers(e) });
    if (type !== "keydown" || e.ctrlKey || e.metaKey) return;
    const text = e.key === "Enter" ? "\r" : e.key.length === 1 ? e.key : "";
    if (text) send({ type: "char", text, modifiers: modifiers(e) });
  };
  const options = { capture: true, passive: true };
  window.addEventListener("mousedown", mouse("mousedown"), options);
  window.addEventListener("mouseup", mouse("mouseup"), options);
  window.addEventListener("keydown", key("keydown"), options);
  window.addEventListener("keyup", key("keyup"), options);
  window.addEventListener("wheel", (e) => {
    if (e.isTrusted) send({ type: "wheel", x: e.clientX, y: e.clientY, deltaX: -e.deltaX, deltaY: -e.deltaY });
  }, options);
})();"#;

/// Engines whose profiles can take part in a synchronizer session.
pub fn is_supported(browser: &str) -> bool {
  matches!(browser, "wayfern" | "cloakbrowser")
}

/// Whether a leader on this engine is watched by script instead of by
/// Wayfern's own capture.
pub fn uses_script(browser: &str) -> bool {
  browser != "wayfern"
}

/// `Page.createIsolatedWorld` parameters for the frame the capture goes in.
pub fn world_params(frame_id: &str) -> Value {
  json!({ "frameId": frame_id, "worldName": WORLD })
}

/// The commands that arm the world `Page.createIsolatedWorld` answered with:
/// the binding first, so it is there by the time the script's listeners fire.
pub fn arm_commands(world: &Value) -> Option<[(&'static str, Value); 2]> {
  let context = world.get("executionContextId")?.as_u64()?;
  Some([
    (
      "Runtime.addBinding",
      json!({ "name": BINDING, "executionContextId": context }),
    ),
    (
      "Runtime.evaluate",
      json!({ "contextId": context, "expression": CAPTURE_SCRIPT }),
    ),
  ])
}

/// Keeps the capture alive across the leader's navigations.
pub struct ScriptCapture {
  next_id: u64,
  /// The `Page.createIsolatedWorld` whose answer is still due.
  awaiting_world: Option<u64>,
}

impl Default for ScriptCapture {
  fn default() -> Self {
    Self {
      next_id: FIRST_COMMAND_ID,
      awaiting_world: None,
    }
  }
}

impl ScriptCapture {
  fn command(&mut self, method: &str, params: Value) -> (u64, String) {
    self.next_id += 1;
    let id = self.next_id;
    (
      id,
      json!({ "id": id, "method": method, "params": params }).to_string(),
    )
  }

  /// Look at one message from the leader. A binding call becomes the input
  /// event it carries, in place. Returns the commands to send in answer: a
  /// new world after a top-level navigation, then its binding and script.
  pub fn on_message(&mut self, message: &mut Value) -> Vec<String> {
    match message.get("method").and_then(Value::as_str) {
      Some("Runtime.bindingCalled") => {
        let params = &message["params"];
        if params["name"] == BINDING {
          if let Some(event) = params["payload"]
            .as_str()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
          {
            *message = json!({ "method": "Wayfern.inputCaptured", "params": event });
          }
        }
        Vec::new()
      }
      Some("Page.frameNavigated") => {
        let frame = &message["params"]["frame"];
        match frame["id"].as_str() {
          Some(frame_id) if frame.get("parentId").is_none() => {
            let (id, command) = self.command("Page.createIsolatedWorld", world_params(frame_id));
            self.awaiting_world = Some(id);
            vec![command]
          }
          _ => Vec::new(),
        }
      }
      Some(_) => Vec::new(),
      None => {
        if message.get("id").and_then(Value::as_u64) != self.awaiting_world {
          return Vec::new();
        }
        self.awaiting_world = None;
        let Some(commands) = message.get("result").and_then(arm_commands) else {
          return Vec::new();
        };
        commands
          .into_iter()
          .map(|(method, params)| self.command(method, params).1)
          .collect()
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sent(command: &str) -> Value {
    serde_json::from_str(command).unwrap()
  }

  #[test]
  fn a_binding_call_becomes_the_input_event_it_carries() {
    let mut capture = ScriptCapture::default();
    let mut message = json!({
      "method": "Runtime.bindingCalled",
      "params": {
        "name": BINDING,
        "payload": r#"{"type":"mousedown","x":120,"y":90,"button":"left","clickCount":1,"modifiers":0}"#,
      },
    });
    assert!(capture.on_message(&mut message).is_empty());
    assert_eq!(message["method"], "Wayfern.inputCaptured");
    let event: crate::synchronizer::CapturedEvent =
      serde_json::from_value(message["params"].clone()).unwrap();
    assert_eq!(event.event_type, "mousedown");
    assert_eq!((event.x, event.y), (Some(120.0), Some(90.0)));
    assert_eq!(event.button.as_deref(), Some("left"));
  }

  #[test]
  fn another_binding_or_a_broken_payload_is_left_alone() {
    let mut capture = ScriptCapture::default();
    for params in [
      json!({ "name": "somethingElse", "payload": "{}" }),
      json!({ "name": BINDING, "payload": "not json" }),
    ] {
      let mut message = json!({ "method": "Runtime.bindingCalled", "params": params });
      let before = message.clone();
      assert!(capture.on_message(&mut message).is_empty());
      assert_eq!(message, before);
    }
  }

  #[test]
  fn a_top_level_navigation_arms_a_new_world() {
    let mut capture = ScriptCapture::default();
    let mut navigated = json!({
      "method": "Page.frameNavigated",
      "params": { "frame": { "id": "FRAME", "url": "https://example.com/" } },
    });
    let commands = capture.on_message(&mut navigated);
    assert_eq!(commands.len(), 1);
    let create = sent(&commands[0]);
    assert_eq!(create["method"], "Page.createIsolatedWorld");
    assert_eq!(create["params"]["frameId"], "FRAME");

    // An answer to some other command changes nothing.
    assert!(capture
      .on_message(&mut json!({ "id": 7, "result": { "executionContextId": 3 } }))
      .is_empty());

    let mut answer = json!({ "id": create["id"], "result": { "executionContextId": 42 } });
    let armed: Vec<Value> = capture
      .on_message(&mut answer)
      .iter()
      .map(|c| sent(c))
      .collect();
    assert_eq!(armed[0]["method"], "Runtime.addBinding");
    assert_eq!(armed[0]["params"]["executionContextId"], 42);
    assert_eq!(armed[1]["method"], "Runtime.evaluate");
    assert_eq!(armed[1]["params"]["contextId"], 42);
    // The world is armed once per answer.
    assert!(capture.on_message(&mut answer).is_empty());
  }

  #[test]
  fn command_ids_fit_what_devtools_accepts() {
    let mut capture = ScriptCapture::default();
    let (id, command) = capture.command("Page.createIsolatedWorld", json!({}));
    assert!(id <= i32::MAX as u64);
    assert_eq!(sent(&command)["id"], id);
  }

  #[test]
  fn a_frame_inside_the_page_does_not_rearm() {
    let mut capture = ScriptCapture::default();
    let mut navigated = json!({
      "method": "Page.frameNavigated",
      "params": { "frame": { "id": "CHILD", "parentId": "FRAME", "url": "https://ads.example/" } },
    });
    assert!(capture.on_message(&mut navigated).is_empty());
  }

  #[test]
  fn the_synchronizer_takes_both_chromium_engines_and_only_those() {
    assert!(is_supported("wayfern") && is_supported("cloakbrowser"));
    assert!(!is_supported("camoufox"));
    assert!(!uses_script("wayfern") && uses_script("cloakbrowser"));
  }
}
