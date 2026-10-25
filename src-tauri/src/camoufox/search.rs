//! Address-bar search in Camoufox profiles.
//!
//! The build ships no search engines: its search configuration carries one
//! inert engine, "None", whose URL is `http://127.0.0.1/`, and its
//! `distribution/policies.json` removes the rest and makes "None" the default,
//! so anything typed in the address bar that is not an address went to the
//! loopback address. Before every launch the policy is rewritten to add real
//! engines, hide "None" and make Google the default. Firefox applies the
//! default once per change of the policy, so a person may pick another engine
//! afterwards, and profiles made under the old policy pick it up too.

use std::path::{Path, PathBuf};

/// Name, keyword and search URL of each engine offered.
const ENGINES: [(&str, &str, &str); 5] = [
  (
    "Google",
    "@google",
    "https://www.google.com/search?q={searchTerms}",
  ),
  (
    "Bing",
    "@bing",
    "https://www.bing.com/search?q={searchTerms}",
  ),
  (
    "DuckDuckGo",
    "@duckduckgo",
    "https://duckduckgo.com/?q={searchTerms}",
  ),
  (
    "Yandex",
    "@yandex",
    "https://yandex.com/search/?text={searchTerms}",
  ),
  (
    "Wikipedia",
    "@wikipedia",
    "https://www.wikipedia.org/search-redirect.php?search={searchTerms}",
  ),
];

fn search_engines_policy() -> serde_json::Value {
  let add: Vec<serde_json::Value> = ENGINES
    .iter()
    .map(|(name, alias, url)| {
      serde_json::json!({ "Name": name, "Alias": alias, "Method": "GET", "URLTemplate": url })
    })
    .collect();
  serde_json::json!({ "Add": add, "Default": ENGINES[0].0, "Remove": ["None"] })
}

/// Where Firefox reads the policies of the build at `executable`.
fn policies_path(executable: &Path) -> Option<PathBuf> {
  let dir = executable.parent()?;
  let distribution = if cfg!(target_os = "macos") {
    dir.parent()?.join("Resources").join("distribution")
  } else {
    dir.join("distribution")
  };
  Some(distribution.join("policies.json"))
}

/// Give the build at `executable` real search engines, keeping its other
/// policies. Written only when it differs, and atomically, since profiles of
/// the same build may be starting at the same time.
pub fn ensure_search_engines(executable: &Path) {
  let Some(path) = policies_path(executable) else {
    return;
  };
  let mut document = std::fs::read_to_string(&path)
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .filter(serde_json::Value::is_object)
    .unwrap_or_else(|| serde_json::json!({}));
  if !document["policies"].is_object() {
    document["policies"] = serde_json::json!({});
  }
  let wanted = search_engines_policy();
  if document["policies"]["SearchEngines"] == wanted {
    return;
  }
  document["policies"]["SearchEngines"] = wanted;

  let written = serde_json::to_string_pretty(&document)
    .map_err(std::io::Error::other)
    .and_then(|text| {
      if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
      }
      let staged = path.with_extension(format!("json.{}", std::process::id()));
      std::fs::write(&staged, text)?;
      std::fs::rename(&staged, &path)
    });
  match written {
    Ok(()) => log::info!("Set Camoufox search engines in {}", path.display()),
    Err(e) => log::warn!(
      "Could not set Camoufox search engines in {}: {e}",
      path.display()
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn build(root: &Path) -> (PathBuf, PathBuf) {
    let executable = root.join("Camoufox.app/Contents/MacOS/camoufox");
    let policies = policies_path(&executable).unwrap();
    std::fs::create_dir_all(policies.parent().unwrap()).unwrap();
    (executable, policies)
  }

  #[test]
  fn replaces_the_loopback_engine_and_keeps_other_policies() {
    let root = tempfile::tempdir().unwrap();
    let (executable, policies) = build(root.path());
    std::fs::write(
      &policies,
      r#"{"policies":{"DisableTelemetry":true,"SearchEngines":{"Default":"None",
        "Add":[{"Name":"None","URLTemplate":"http://127.0.0.1"}]}}}"#,
    )
    .unwrap();

    ensure_search_engines(&executable);

    let written: serde_json::Value =
      serde_json::from_str(&std::fs::read_to_string(&policies).unwrap()).unwrap();
    assert_eq!(written["policies"]["DisableTelemetry"], true);
    let search = &written["policies"]["SearchEngines"];
    assert_eq!(search["Default"], "Google");
    assert_eq!(search["Remove"], serde_json::json!(["None"]));
    let names: Vec<&str> = search["Add"]
      .as_array()
      .unwrap()
      .iter()
      .map(|engine| engine["Name"].as_str().unwrap())
      .collect();
    assert!(names.contains(&"Google"));
    assert!(!names.contains(&"None"));
    assert!(search["Add"]
      .as_array()
      .unwrap()
      .iter()
      .all(|engine| engine["URLTemplate"]
        .as_str()
        .unwrap()
        .starts_with("https://")));
  }

  #[test]
  fn leaves_a_policy_that_already_has_them_untouched() {
    let root = tempfile::tempdir().unwrap();
    let (executable, policies) = build(root.path());
    ensure_search_engines(&executable);
    let first = std::fs::read_to_string(&policies).unwrap();
    std::fs::write(&policies, format!("{first}\n")).unwrap();

    ensure_search_engines(&executable);

    assert_eq!(
      std::fs::read_to_string(&policies).unwrap(),
      format!("{first}\n")
    );
  }
}
