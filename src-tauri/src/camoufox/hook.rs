//! Shared plumbing for the blocks Owl adds to a Camoufox build's `camoufox.cfg`
//! (Firefox autoconfig). Each block carries its own begin/end markers, so it is
//! replaced on its own and the other hooks stay as they are.

use std::path::Path;

/// The pref file that lets `camoufox.cfg` run the browser's own code
/// (`general.config.sandbox_enabled` off). Written by every hook installer;
/// the content is the same, so the second writer finds nothing to change.
pub(crate) const AUTOCONFIG_PREFS: &str =
  "// Owl: lets camoufox.cfg run the browser's own code (camoufox/hook.rs).\n\
pref(\"general.config.sandbox_enabled\", false);\n";

/// `cfg` with the block between `begin` and `end` replaced by
/// `begin`..`body`..`end`, or appended when it is not there yet. Every earlier
/// copy is removed, so re-running any number of times leaves one block and the
/// content outside the markers untouched.
pub(crate) fn upsert_block(cfg: &str, begin: &str, end: &str, body: &str) -> String {
  let block = format!("{begin}\n{body}{end}\n");
  let mut kept = String::with_capacity(cfg.len() + block.len());
  let mut rest = cfg;
  let mut insert_at = None;
  while let Some(start) = rest.find(begin) {
    let after = &rest[start + begin.len()..];
    let Some(end_offset) = after.find(end) else {
      break;
    };
    kept.push_str(&rest[..start]);
    insert_at.get_or_insert(kept.len());
    // The block is inserted with a trailing newline; consuming the one after
    // the old end marker is what makes a second run leave the file as it was.
    let mut block_end = start + begin.len() + end_offset + end.len();
    if rest[block_end..].starts_with('\n') {
      block_end += 1;
    }
    rest = &rest[block_end..];
  }
  kept.push_str(rest);
  match insert_at {
    Some(at) => {
      kept.insert_str(at, &block);
      kept
    }
    None => {
      let trimmed = kept.trim_end();
      if trimmed.is_empty() {
        block
      } else {
        format!("{trimmed}\n\n{block}")
      }
    }
  }
}

/// Make sure the build at `resources` lets its `camoufox.cfg` run scripts.
pub(crate) fn ensure_autoconfig_pref(resources: &Path) -> std::io::Result<bool> {
  super::write_if_changed(
    &resources.join("defaults").join("pref").join("owl.js"),
    AUTOCONFIG_PREFS,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  const BEGIN: &str = "// test hook (begin)";
  const END: &str = "// test hook (end)";

  #[test]
  fn replaces_a_block_in_place_and_keeps_the_rest() {
    let cfg = "// base\n\n// test hook (begin)\nold();\n// test hook (end)\n\n// after\n";
    let updated = upsert_block(cfg, BEGIN, END, "new();\n");
    assert!(updated.starts_with("// base\n\n"));
    assert!(updated.ends_with("// after\n"));
    assert_eq!(updated.matches(BEGIN).count(), 1);
    assert!(updated.contains("new();"));
    assert!(!updated.contains("old();"));
    assert_eq!(upsert_block(&updated, BEGIN, END, "new();\n"), updated);
  }

  #[test]
  fn appends_once_and_collapses_duplicates() {
    let once = upsert_block("// base\n", BEGIN, END, "body();\n");
    assert_eq!(
      once,
      "// base\n\n// test hook (begin)\nbody();\n// test hook (end)\n"
    );
    let twice = format!("{once}\n{once}");
    let collapsed = upsert_block(&twice, BEGIN, END, "body();\n");
    assert_eq!(collapsed.matches(BEGIN).count(), 1);
    assert_eq!(collapsed.matches("body();").count(), 1);
    assert_eq!(upsert_block(&collapsed, BEGIN, END, "body();\n"), collapsed);
  }

  #[test]
  fn an_unclosed_block_is_left_alone() {
    let cfg = "// test hook (begin)\nhalf a block\n";
    let updated = upsert_block(cfg, BEGIN, END, "body();\n");
    assert!(updated.starts_with(cfg));
    assert_eq!(updated.matches(BEGIN).count(), 2);
  }

  #[test]
  fn two_hooks_do_not_replace_each_other() {
    const OTHER_BEGIN: &str = "// other hook (begin)";
    const OTHER_END: &str = "// other hook (end)";
    let both = upsert_block(
      &upsert_block("// base\n", BEGIN, END, "first();\n"),
      OTHER_BEGIN,
      OTHER_END,
      "second();\n",
    );
    assert!(both.contains("first();"));
    assert!(both.contains("second();"));
    assert_eq!(both.matches(BEGIN).count(), 1);
    assert_eq!(both.matches(OTHER_BEGIN).count(), 1);
    assert_eq!(
      upsert_block(&both, BEGIN, END, "first();\n"),
      both,
      "replacing one hook leaves the other one where it was"
    );
  }
}
