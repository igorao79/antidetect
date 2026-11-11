//! A per-profile macOS application bundle for Camoufox.
//!
//! macOS gives every `.app` bundle one Dock tile: the title and icon come from
//! the bundle's `Info.plist`, not from the running profile. Launching one
//! `Camoufox.app` for ten profiles therefore shows ten identical tiles and the
//! Dock label says "Camoufox".
//!
//! This module makes a clone of the Camoufox bundle per profile (an APFS clone
//! when the volume supports it, so the 2.4 GB build shares its blocks), gives
//! it the profile's name and a generated icon, and ad-hoc signs the outer
//! bundle. The launch uses the clone's executable; the browser keeps its
//! profile and build, but macOS now sees a separate application per profile.
//!
//! The clone is cache, not profile data: it lives under the app cache, is
//! rebuilt when the source build or the profile's name/colour changes, and is
//! removed with the profile. Only macOS is handled: Windows taskbar grouping
//! and Linux window classes need different mechanisms, and the window badge,
//! title and chrome colour in `profile_identity` stay the common layer.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::profile::BrowserProfile;

/// Bumped when the bundle layout or the icon renderer changes, so existing
/// clones are rebuilt on the next launch.
const FORMAT_VERSION: u32 = 1;
const CACHE_DIR: &str = "camoufox-profile-apps";
const APP_NAME: &str = "Owl.app";
const STAMP_FILE: &str = ".owl-profile-app.json";
const ICON_FILE: &str = "owl-profile.icns";

/// Cloning and ad-hoc signing one bundle is a few seconds of disk and CPU work.
/// Serialise it across profiles: bulk launch otherwise runs ten codesign jobs
/// at once, and two launches of one profile could replace each other's bundle.
static PREPARE_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
  std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BundleStamp {
  format: u32,
  source: String,
  info_modified: u64,
  executable_modified: u64,
  name: String,
  color: String,
  bundle_id: String,
}

/// The executable a launch must use. On macOS this is the clone's
/// `Contents/MacOS/camoufox`; everywhere else, and whenever the clone cannot be
/// prepared, it is the original executable.
pub fn prepare_profile_app(
  profile: &BrowserProfile,
  source_executable: &Path,
) -> Result<PathBuf, String> {
  if !cfg!(target_os = "macos") {
    return Ok(source_executable.to_path_buf());
  }
  let _preparing = PREPARE_LOCK
    .lock()
    .map_err(|_| "Per-profile app lock is poisoned".to_string())?;
  let Some(source_bundle) = bundle_root(source_executable) else {
    return Ok(source_executable.to_path_buf());
  };
  if source_bundle.starts_with(cache_root()) {
    return Ok(source_executable.to_path_buf());
  }

  let name = display_name(&profile.name);
  let color = crate::camoufox::profile_identity::profile_color(profile);
  let bundle_id = bundle_identifier(profile, &name, &color);
  let wanted = BundleStamp {
    format: FORMAT_VERSION,
    source: source_bundle.to_string_lossy().to_string(),
    info_modified: modified_secs(&source_bundle.join("Contents/Info.plist")),
    executable_modified: modified_secs(&source_bundle.join("Contents/MacOS/camoufox")),
    name: name.clone(),
    color: color.clone(),
    bundle_id: bundle_id.clone(),
  };

  let dir = cache_root().join(profile.id.to_string());
  let app = dir.join(APP_NAME);
  let executable = app.join("Contents/MacOS/camoufox");
  if read_stamp(&dir).as_ref() == Some(&wanted) && executable.is_file() {
    return Ok(executable);
  }

  if dir.exists() {
    std::fs::remove_dir_all(&dir)
      .map_err(|e| format!("Failed to replace profile app {}: {e}", dir.display()))?;
  }
  std::fs::create_dir_all(&dir)
    .map_err(|e| format!("Failed to create profile app cache {}: {e}", dir.display()))?;

  let prepared = (|| -> Result<(), String> {
    clone_bundle(&source_bundle, &app)?;
    set_bundle_identity(&app, &name, &bundle_id)?;
    let svg = icon_svg(
      &name,
      &crate::camoufox::profile_identity::badge_from_name(&name),
      &color,
    );
    if let Err(e) = write_icns(&app, &svg, &dir) {
      log::warn!("Profile app icon not generated, keeping Firefox icon err=\"{e}\"");
      copy_fallback_icon(&app, &source_bundle);
    }
    remove_quarantine(&app);
    sign_bundle(&app)?;
    write_stamp(&dir, &wanted)?;
    Ok(())
  })();

  if let Err(e) = prepared {
    let _ = std::fs::remove_dir_all(&dir);
    return Err(e);
  }
  log::info!(
    "Prepared per-profile app profile={} bundle_id={}",
    profile.id,
    bundle_id
  );
  Ok(executable)
}

/// Remove the cached bundle of a deleted profile. Best effort: the cache is
/// rebuildable, and a failure must not make the profile deletion fail.
pub fn remove_profile_app(profile_id: &str) {
  if !cfg!(target_os = "macos") {
    return;
  }
  if uuid::Uuid::parse_str(profile_id).is_err() {
    return;
  }
  let Ok(_removing) = PREPARE_LOCK.lock() else {
    return;
  };
  let dir = cache_root().join(profile_id);
  if !dir.exists() {
    return;
  }
  match std::fs::remove_dir_all(&dir) {
    Ok(()) => log::debug!("Removed per-profile app profile={profile_id}"),
    Err(e) => log::warn!("Per-profile app not removed profile={profile_id} err=\"{e}\""),
  }
}

fn cache_root() -> PathBuf {
  crate::app_dirs::cache_dir().join(CACHE_DIR)
}

fn display_name(raw: &str) -> String {
  let name: String = raw
    .trim()
    .chars()
    .filter(|c| !c.is_control())
    .take(64)
    .collect();
  if name.trim().is_empty() {
    "Owl Profile".to_string()
  } else {
    name
  }
}

fn bundle_root(executable: &Path) -> Option<PathBuf> {
  let mut current = executable;
  while let Some(parent) = current.parent() {
    if parent.extension().and_then(|e| e.to_str()) == Some("app")
      && parent.join("Contents/MacOS").is_dir()
    {
      return Some(parent.to_path_buf());
    }
    current = parent;
  }
  None
}

fn modified_secs(path: &Path) -> u64 {
  std::fs::metadata(path)
    .and_then(|m| m.modified())
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_secs())
    .unwrap_or(0)
}

fn read_stamp(dir: &Path) -> Option<BundleStamp> {
  let data = std::fs::read(dir.join(STAMP_FILE)).ok()?;
  serde_json::from_slice(&data).ok()
}

fn write_stamp(dir: &Path, stamp: &BundleStamp) -> Result<(), String> {
  let data = serde_json::to_vec_pretty(stamp).map_err(|e| e.to_string())?;
  std::fs::write(dir.join(STAMP_FILE), data).map_err(|e| e.to_string())
}

fn bundle_identifier(profile: &BrowserProfile, name: &str, color: &str) -> String {
  // A content hash makes a rename or recolour a new bundle identity, so macOS
  // does not keep serving the old cached Dock icon to the same identifier.
  let mut hash: u32 = 2166136261;
  hash ^= FORMAT_VERSION;
  hash = hash.wrapping_mul(16777619);
  for byte in name
    .bytes()
    .chain(color.bytes())
    .chain(profile.id.as_bytes().iter().copied())
  {
    hash ^= u32::from(byte);
    hash = hash.wrapping_mul(16777619);
  }
  let id = profile.id.to_string().replace('-', "");
  format!("com.owl.camoufox.{id}.{hash:08x}")
}

fn clone_bundle(source: &Path, destination: &Path) -> Result<(), String> {
  // `-c` is clonefile(2) on APFS: cheap, copy-on-write. A different filesystem
  // falls back to a full copy rather than failing the launch.
  let cloned = Command::new("/bin/cp")
    .arg("-cR")
    .arg(source)
    .arg(destination)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  if cloned {
    return Ok(());
  }
  log::warn!(
    "APFS clone unavailable, copying Camoufox.app for a profile source={}",
    source.display()
  );
  let copied = Command::new("/bin/cp")
    .arg("-R")
    .arg(source)
    .arg(destination)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  if copied {
    Ok(())
  } else {
    Err("Failed to copy Camoufox.app".to_string())
  }
}

fn set_bundle_identity(app: &Path, name: &str, bundle_id: &str) -> Result<(), String> {
  let plist = app.join("Contents/Info.plist");
  plutil_set(&plist, "CFBundleDisplayName", name)?;
  plutil_set(&plist, "CFBundleName", name)?;
  plutil_set(&plist, "CFBundleIdentifier", bundle_id)?;
  plutil_set(&plist, "CFBundleIconFile", ICON_FILE)?;
  // `CFBundleIconName` wins over `CFBundleIconFile` when both exist.
  let _ = Command::new("/usr/bin/plutil")
    .args(["-remove", "CFBundleIconName"])
    .arg(&plist)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status();
  Ok(())
}

fn plutil_set(plist: &Path, key: &str, value: &str) -> Result<(), String> {
  let replaced = Command::new("/usr/bin/plutil")
    .args(["-replace", key, "-string", value])
    .arg(plist)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  if replaced {
    return Ok(());
  }
  let inserted = Command::new("/usr/bin/plutil")
    .args(["-insert", key, "-string", value])
    .arg(plist)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  if inserted {
    Ok(())
  } else {
    Err(format!("plutil could not set {key}"))
  }
}

fn icon_svg(name: &str, badge: &str, color: &str) -> String {
  let escaped_name = xml_escape(&name.chars().take(14).collect::<String>());
  let escaped_badge = xml_escape(badge);
  format!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="224" fill="#0b0f14"/>
  <rect x="56" y="56" width="912" height="912" rx="190" fill="none" stroke="{color}" stroke-width="26"/>
  <circle cx="512" cy="420" r="250" fill="{color}" opacity="0.16"/>
  <text x="512" y="540" text-anchor="middle" font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-size="330" font-weight="700" fill="#ffffff">{escaped_badge}</text>
  <text x="512" y="760" text-anchor="middle" font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-size="66" font-weight="500" fill="#c7ccd6">{escaped_name}</text>
</svg>"##
  )
}

fn xml_escape(value: &str) -> String {
  value
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
    .replace('\'', "&apos;")
}

fn render_icon(svg: &str, size: u32) -> Result<Vec<u8>, String> {
  let mut options = resvg::usvg::Options::default();
  options.fontdb_mut().load_system_fonts();
  let tree = resvg::usvg::Tree::from_str(svg, &options)
    .map_err(|e| format!("Failed to parse profile icon SVG: {e}"))?;
  let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
    .ok_or_else(|| format!("Failed to allocate {size}px profile icon"))?;
  let scale = size as f32 / 1024.0;
  resvg::render(
    &tree,
    resvg::tiny_skia::Transform::from_scale(scale, scale),
    &mut pixmap.as_mut(),
  );
  pixmap
    .encode_png()
    .map_err(|e| format!("Failed to encode profile icon: {e}"))
}

fn write_icns(app: &Path, svg: &str, cache_dir: &Path) -> Result<(), String> {
  let iconset = cache_dir.join("owl-profile.iconset");
  if iconset.exists() {
    std::fs::remove_dir_all(&iconset).map_err(|e| e.to_string())?;
  }
  std::fs::create_dir_all(&iconset).map_err(|e| e.to_string())?;
  let entries = [
    (16, "icon_16x16.png"),
    (32, "icon_16x16@2x.png"),
    (32, "icon_32x32.png"),
    (64, "icon_32x32@2x.png"),
    (128, "icon_128x128.png"),
    (256, "icon_128x128@2x.png"),
    (256, "icon_256x256.png"),
    (512, "icon_256x256@2x.png"),
    (512, "icon_512x512.png"),
    (1024, "icon_512x512@2x.png"),
  ];
  // Render the largest icon once (loading system fonts is the expensive part)
  // and let sips produce the smaller representations iconutil expects.
  let base = iconset.join("base-1024.png");
  std::fs::write(&base, render_icon(svg, 1024)?).map_err(|e| e.to_string())?;
  for (size, file) in entries {
    let destination = iconset.join(file);
    if size == 1024 {
      std::fs::copy(&base, &destination).map_err(|e| e.to_string())?;
      continue;
    }
    let status = Command::new("/usr/bin/sips")
      .args(["-z", &size.to_string(), &size.to_string()])
      .arg(&base)
      .args(["--out"])
      .arg(&destination)
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .map(|s| s.success())
      .unwrap_or(false);
    if !status {
      return Err(format!("sips could not resize the {size}px profile icon"));
    }
  }
  let _ = std::fs::remove_file(&base);
  let output = app.join("Contents/Resources").join(ICON_FILE);
  if let Some(parent) = output.parent() {
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  let status = Command::new("/usr/bin/iconutil")
    .arg("-c")
    .arg("icns")
    .arg(&iconset)
    .arg("-o")
    .arg(&output)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  let _ = std::fs::remove_dir_all(&iconset);
  if !status {
    return Err("iconutil could not build owl-profile.icns".to_string());
  }
  Ok(())
}

fn copy_fallback_icon(app: &Path, source: &Path) {
  let source_icon = source.join("Contents/Resources/firefox.icns");
  let destination = app.join("Contents/Resources").join(ICON_FILE);
  if let Some(parent) = destination.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  if let Err(e) = std::fs::copy(&source_icon, &destination) {
    log::warn!("Fallback Camoufox icon not copied err=\"{e}\"");
  }
}

fn remove_quarantine(app: &Path) {
  // A clone of a locally downloaded build has no quarantine, but a copied
  // build from a browser download might; removing it keeps the local clone
  // launchable without a Gatekeeper prompt for an app the person already has.
  let _ = Command::new("/usr/bin/xattr")
    .args(["-dr", "com.apple.quarantine"])
    .arg(app)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status();
}

fn sign_bundle(app: &Path) -> Result<(), String> {
  let signed = Command::new("/usr/bin/codesign")
    .args([
      "--force",
      "--sign",
      "-",
      "--preserve-metadata=entitlements,flags",
    ])
    .arg(app)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  if signed {
    return Ok(());
  }
  let retried = Command::new("/usr/bin/codesign")
    .args(["--force", "--sign", "-"])
    .arg(app)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false);
  if retried {
    Ok(())
  } else {
    Err("codesign could not sign the per-profile Camoufox app".to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn profile() -> BrowserProfile {
    BrowserProfile {
      name: "Client EU & Co".to_string(),
      window_color: Some("#a1b2c3".to_string()),
      ..Default::default()
    }
  }

  #[test]
  fn bundle_root_finds_the_app_above_contents_macos() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("Camoufox.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    let executable = app.join("Contents/MacOS/camoufox");
    std::fs::write(&executable, b"binary").unwrap();
    assert_eq!(bundle_root(&executable), Some(app));
  }

  #[test]
  fn a_plain_binary_has_no_bundle() {
    let tmp = tempfile::tempdir().unwrap();
    let executable = tmp.path().join("camoufox");
    std::fs::write(&executable, b"binary").unwrap();
    assert_eq!(bundle_root(&executable), None);
  }

  #[test]
  fn the_icon_escapes_the_profile_name_and_keeps_the_badge() {
    let svg = icon_svg("A & <B>", "AB", "#a1b2c3");
    assert!(svg.contains("A &amp; &lt;B&gt;"));
    assert!(svg.contains(">AB<"));
    assert!(svg.contains("#a1b2c3"));
  }

  #[test]
  fn a_renamed_or_recoloured_profile_gets_a_new_bundle_identity() {
    let p = profile();
    let first = bundle_identifier(&p, "Client EU", "#a1b2c3");
    let renamed = bundle_identifier(&p, "Client UK", "#a1b2c3");
    let recoloured = bundle_identifier(&p, "Client EU", "#112233");
    assert_ne!(first, renamed);
    assert_ne!(first, recoloured);
    assert!(first.starts_with("com.owl.camoufox."));
  }

  #[test]
  fn the_icon_renderer_produces_a_png_at_the_requested_size() {
    let svg = icon_svg("Client EU", "CE", "#a1b2c3");
    let png = render_icon(&svg, 64).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(png.len() > 100);
  }

  /// A real Camoufox build can be cloned and signed. The test is opt-in: it
  /// copies a 2.4 GB app and runs codesign, so CI and a normal `cargo test`
  /// must not pay for it. Run with:
  ///   OWL_TEST_CAMOUFOX_APP=/path/Camoufox.app cargo test --lib \
  ///     camoufox::profile_app::tests::a_real_camoufox_bundle_can_be_prepared
  #[test]
  fn a_real_camoufox_bundle_can_be_prepared() {
    let Some(source) = std::env::var_os("OWL_TEST_CAMOUFOX_APP") else {
      return;
    };
    let cache = tempfile::tempdir().unwrap();
    let _guard = crate::app_dirs::set_test_cache_dir(cache.path().to_path_buf());
    let p = profile();
    let executable = prepare_profile_app(
      &p,
      std::path::Path::new(&source)
        .join("Contents/MacOS/camoufox")
        .as_path(),
    )
    .unwrap();
    assert!(executable.is_file());
    let app = executable
      .ancestors()
      .find(|path| path.extension().and_then(|e| e.to_str()) == Some("app"))
      .unwrap();
    let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
    assert!(plist.contains("Client EU &amp; Co") || plist.contains("Client EU & Co"));
    assert!(!plist.contains("CFBundleIconName"));
    let icon = std::fs::read(app.join("Contents/Resources/owl-profile.icns")).unwrap();
    let firefox = std::fs::read(
      std::path::Path::new(&source)
        .join("Contents/Resources/firefox.icns")
        .as_path(),
    )
    .unwrap();
    assert_ne!(icon, firefox, "the generated icon fell back to Firefox's");
    let output = Command::new(&executable).arg("--version").output().unwrap();
    assert!(output.status.success(), "{:?}", output);
  }
}
