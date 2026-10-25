//! Bundled fonts of the Camoufox build that clash with this Mac's own fonts.
//!
//! Camoufox carries the fonts of every OS it can pose as (`Resources/fonts`,
//! activated by `gfx.bundled-fonts.activate` in camoufox.cfg), among them its
//! own copies of macOS fonts (SF, Helvetica, Lucida Grande) and of Windows
//! fonts macOS also has (Arial, Georgia, Tahoma), from other versions than the
//! installed ones. A profile posing as another OS shapes text with the bundled
//! file while WebRender draws the glyphs of the installed font of the same name,
//! so the browser's tabs and address bar, and text that falls back to such a
//! font (Cyrillic in Arial), came out as other characters. Those bundled files
//! are moved to `Resources/fonts-shadowed`, which Camoufox does not read; the
//! installed font serves both. Fonts this Mac does not have stay bundled.
//!
//! The check runs once per build and macOS version; after a macOS update the
//! shadowed files go back and the check runs again.

use std::collections::HashSet;
use std::path::Path;

const SHADOWED: &str = "fonts-shadowed";
const MARKER: &str = ".owl-checked";

fn macos_build() -> String {
  std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
    .ok()
    .and_then(|plist| {
      let after = plist.split("<key>ProductBuildVersion</key>").nth(1)?;
      let value = after.split("<string>").nth(1)?.split("</string>").next()?;
      Some(value.trim().to_string())
    })
    .unwrap_or_default()
}

/// The installed fonts: their PostScript names and lowercased file names.
/// Hidden system fonts (`.SFNS-Regular`) are only found by their files.
fn installed_fonts() -> (HashSet<String>, HashSet<String>) {
  let mut db = fontdb::Database::new();
  db.load_system_fonts();
  let mut names = HashSet::new();
  let mut files = HashSet::new();
  for face in db.faces() {
    names.insert(face.post_script_name.clone());
    if let fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) = &face.source {
      if let Some(name) = path.file_name() {
        files.insert(name.to_string_lossy().to_lowercase());
      }
    }
  }
  (names, files)
}

/// The file name Camoufox stored a font under, without its `<n>__` prefix.
fn original_file_name(bundled: &str) -> &str {
  match bundled.split_once("__") {
    Some((index, name)) if index.chars().all(|c| c.is_ascii_digit()) => name,
    _ => bundled,
  }
}

fn clashes(file: &Path, names: &HashSet<String>, files: &HashSet<String>) -> bool {
  let Some(name) = file.file_name().map(|n| n.to_string_lossy().to_string()) else {
    return false;
  };
  if files.contains(&original_file_name(&name).to_lowercase()) {
    return true;
  }
  let mut db = fontdb::Database::new();
  if db.load_font_file(file).is_err() {
    return false;
  }
  let clash = db
    .faces()
    .any(|face| names.contains(&face.post_script_name));
  clash
}

fn move_all(from: &Path, to: &Path, keep: impl Fn(&Path) -> bool) -> std::io::Result<usize> {
  let mut moved = 0;
  for entry in std::fs::read_dir(from)? {
    let path = entry?.path();
    if !path.is_file() || keep(&path) {
      continue;
    }
    if let Some(name) = path.file_name() {
      std::fs::rename(&path, to.join(name))?;
      moved += 1;
    }
  }
  Ok(moved)
}

fn shadow(
  resources: &Path,
  names: &HashSet<String>,
  files: &HashSet<String>,
) -> std::io::Result<usize> {
  let fonts = resources.join("fonts");
  let shadowed = resources.join(SHADOWED);
  std::fs::create_dir_all(&shadowed)?;
  // A file shadowed under another macOS version gets checked again.
  move_all(&shadowed, &fonts, |path| {
    path.file_name().is_some_and(|name| name == MARKER)
  })?;
  move_all(&fonts, &shadowed, |path| !clashes(path, names, files))
}

/// Keep the bundled fonts of the build at `executable` from standing in for
/// fonts this Mac has. Blocking: it reads every bundled and installed font
/// the first time a build runs on a macOS version.
pub fn shadow_installed_fonts(executable: &Path) {
  if !cfg!(target_os = "macos") {
    return;
  }
  let Some(resources) = super::resources_dir(executable) else {
    return;
  };
  if !resources.join("fonts").is_dir() {
    return;
  }
  let marker = resources.join(SHADOWED).join(MARKER);
  let build = macos_build();
  if std::fs::read_to_string(&marker).is_ok_and(|done| done == build) {
    return;
  }
  let (names, files) = installed_fonts();
  match shadow(&resources, &names, &files).and_then(|moved| {
    std::fs::write(&marker, &build)?;
    Ok(moved)
  }) {
    Ok(moved) => log::info!(
      "Shadowed {moved} bundled Camoufox fonts that this Mac has installed ({})",
      resources.display()
    ),
    Err(e) => log::warn!(
      "Could not shadow bundled Camoufox fonts in {}: {e}",
      resources.display()
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_the_name_camoufox_stored_a_font_under() {
    assert_eq!(original_file_name("42__SFNS.ttf"), "SFNS.ttf");
    assert_eq!(
      original_file_name("944__Arial Narrow Italic.ttf"),
      "Arial Narrow Italic.ttf"
    );
    assert_eq!(original_file_name("PingFang.ttc"), "PingFang.ttc");
    assert_eq!(
      original_file_name("openadd-C059-Italic.t1"),
      "openadd-C059-Italic.t1"
    );
  }

  #[test]
  fn shadows_fonts_by_file_name_and_brings_them_back_for_another_check() {
    let root = tempfile::tempdir().unwrap();
    let resources = root.path();
    let fonts = resources.join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    std::fs::write(fonts.join("42__SFNS.ttf"), b"not a font").unwrap();
    std::fs::write(fonts.join("110__calibriz.ttf"), b"not a font").unwrap();
    let files: HashSet<String> = ["sfns.ttf".to_string()].into();

    assert_eq!(shadow(resources, &HashSet::new(), &files).unwrap(), 1);
    assert!(resources.join(SHADOWED).join("42__SFNS.ttf").exists());
    assert!(fonts.join("110__calibriz.ttf").exists());

    // Another macOS without that font: it goes back.
    assert_eq!(
      shadow(resources, &HashSet::new(), &HashSet::new()).unwrap(),
      0
    );
    assert!(fonts.join("42__SFNS.ttf").exists());
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn finds_this_macs_interface_font() {
    let (_, files) = installed_fonts();
    assert!(files.contains("sfns.ttf"));
  }
}
