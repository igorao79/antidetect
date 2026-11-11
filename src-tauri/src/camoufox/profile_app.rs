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
//! rebuilt when the source build, its hooked resources or the profile's
//! name/colour change, and is removed with the profile. Only macOS is
//! handled: Windows taskbar grouping
//! and Linux window classes need different mechanisms, and the window badge,
//! title and chrome colour in `profile_identity` stay the common layer.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::MutexGuard;

use crate::profile::BrowserProfile;

/// Bumped when the bundle layout or the icon renderer changes, so existing
/// clones are rebuilt on the next launch.
const FORMAT_VERSION: u32 = 3;
const CACHE_DIR: &str = "camoufox-profile-apps";
const APP_NAME: &str = "Owl.app";
const STAMP_FILE: &str = ".owl-profile-app.json";
const ICON_FILE: &str = "owl-profile.icns";
const EXECUTABLE: &str = "Contents/MacOS/camoufox";
/// Prefix of a clone moved aside for deletion (`remove_profile_app`).
const TRASH_PREFIX: &str = ".trash-";
/// Files of `Contents/Resources` that the launch hooks write into the shared
/// build before it is cloned (`search.rs`, `hook.rs`, `last_window.rs`,
/// `profile_identity.rs`, `bundled_fonts.rs`). A clone copies them once, so
/// the stamp carries their contents: a hook changed by a later Owl release, or
/// fonts shadowed again after a macOS update, rebuilds the clone.
const HOOKED_RESOURCES: &[&str] = &[
  "camoufox.cfg",
  "distribution/policies.json",
  "defaults/pref/owl.js",
  "fonts-shadowed/.owl-checked",
];

/// Cloning and ad-hoc signing one bundle is a few seconds of disk and CPU work.
/// Serialise it across profiles: bulk launch otherwise runs ten codesign jobs
/// at once, and two launches of one profile could replace each other's bundle.
static PREPARE_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
  std::sync::LazyLock::new(|| std::sync::Mutex::new(()));
static TRASH_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BundleStamp {
  format: u32,
  source: String,
  info_modified: u64,
  executable_modified: u64,
  resources: String,
  name: String,
  color: String,
  bundle_id: String,
}

/// What the startup sweep decides for one entry of the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloneFate {
  Keep,
  Remove,
}

/// The profiles the sweep compares the cache with: every profile id, and the
/// ids of profiles whose browser runs (from the clone it must keep).
#[derive(Debug, Default)]
struct ProfileStates {
  existing: HashSet<String>,
  running: HashSet<String>,
}

/// The guard carries no data, so a panic while it was held leaves nothing
/// inconsistent behind it.
fn prepare_lock() -> MutexGuard<'static, ()> {
  PREPARE_LOCK
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Patch the shared build with `patch_build`, then return the executable a
/// launch must use. On macOS this is the clone's `Contents/MacOS/camoufox`;
/// everywhere else, and whenever the clone cannot be prepared, it is the
/// original executable. Both steps run under one lock, so two launches never
/// patch the build at once and a clone never copies a half-patched build.
pub fn prepare_profile_app(
  profile: &BrowserProfile,
  source_executable: &Path,
  patch_build: impl FnOnce(),
) -> Result<PathBuf, String> {
  let _preparing = prepare_lock();
  patch_build();
  if !cfg!(target_os = "macos") {
    return Ok(source_executable.to_path_buf());
  }
  prepare_locked(profile, source_executable)
}

fn prepare_locked(profile: &BrowserProfile, source_executable: &Path) -> Result<PathBuf, String> {
  let Some(source_bundle) = bundle_root(source_executable) else {
    return Ok(source_executable.to_path_buf());
  };
  if source_bundle.starts_with(cache_root()) {
    return Ok(source_executable.to_path_buf());
  }

  let name = display_name(&profile.name);
  let color = crate::camoufox::profile_identity::profile_color(profile);
  let bundle_id = bundle_identifier(profile);
  let wanted = BundleStamp {
    format: FORMAT_VERSION,
    source: source_bundle.to_string_lossy().to_string(),
    info_modified: modified_secs(&source_bundle.join("Contents/Info.plist")),
    executable_modified: modified_secs(&source_bundle.join(EXECUTABLE)),
    resources: hooked_resources_digest(&source_bundle),
    name: name.clone(),
    color: color.clone(),
    bundle_id: bundle_id.clone(),
  };

  let dir = cache_root().join(profile.id.to_string());
  let app = dir.join(APP_NAME);
  let executable = app.join(EXECUTABLE);
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
    register_bundle(&app);
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
/// rebuildable, and a failure must not make the profile deletion fail. It does
/// not wait for a launch preparing another clone: the directory is moved out
/// of the way first, and a leftover is deleted by the next startup sweep.
pub fn remove_profile_app(profile_id: &str) {
  if !cfg!(target_os = "macos") {
    return;
  }
  if uuid::Uuid::parse_str(profile_id).is_err() {
    return;
  }
  let dir = cache_root().join(profile_id);
  if std::fs::symlink_metadata(&dir).is_err() {
    return;
  }
  let target = move_to_trash(&dir);
  match std::fs::remove_dir_all(&target) {
    Ok(()) => log::debug!("Removed per-profile app profile={profile_id}"),
    Err(e) => log::warn!("Per-profile app not removed profile={profile_id} err=\"{e}\""),
  }
}

/// Remove the clones no launch can use again: of a deleted profile, of a
/// Camoufox build that is gone (an APFS clone keeps the old build's blocks
/// allocated), without a readable stamp, and trash left by an interrupted
/// removal. A profile whose browser runs keeps its clone.
pub fn sweep_profile_apps() {
  if !cfg!(target_os = "macos") {
    return;
  }
  let started = std::time::Instant::now();
  let doomed = {
    // Nothing is half-prepared while this is held, so a clone without a
    // stamp is a failed one, and no profile can be launching from a clone
    // this decides to remove.
    let _preparing = prepare_lock();
    let Ok(entries) = std::fs::read_dir(cache_root()) else {
      return;
    };
    let profiles = match current_profile_states() {
      Ok(profiles) => profiles,
      Err(e) => {
        log::warn!("Per-profile app sweep skipped err=\"{e}\"");
        return;
      }
    };
    entries
      .filter_map(Result::ok)
      .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
      .map(|entry| entry.path())
      .filter(|dir| clone_fate(dir, &profiles) == CloneFate::Remove)
      .map(|dir| move_to_trash(&dir))
      .collect::<Vec<_>>()
  };
  let removed = doomed
    .iter()
    .filter(|dir| match std::fs::remove_dir_all(dir) {
      Ok(()) => true,
      Err(e) => {
        log::debug!("Per-profile app not swept err=\"{e}\"");
        false
      }
    })
    .count();
  let elapsed_ms = started.elapsed().as_millis();
  if removed > 0 {
    log::info!("Swept per-profile apps removed={removed} elapsed_ms={elapsed_ms}");
  } else {
    log::debug!("Swept per-profile apps removed=0 elapsed_ms={elapsed_ms}");
  }
}

fn current_profile_states() -> Result<ProfileStates, String> {
  let profiles = crate::profile::ProfileManager::instance()
    .list_profiles()
    .map_err(|e| e.to_string())?;
  let running = profiles
    .iter()
    .filter(|p| {
      p.process_id
        .is_some_and(crate::proxy_storage::is_process_running)
    })
    .map(|p| p.id.to_string())
    .collect();
  let existing = profiles.iter().map(|p| p.id.to_string()).collect();
  Ok(ProfileStates { existing, running })
}

fn clone_fate(dir: &Path, profiles: &ProfileStates) -> CloneFate {
  let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
    return CloneFate::Remove;
  };
  if name.starts_with(TRASH_PREFIX) {
    return CloneFate::Remove;
  }
  if profiles.running.contains(name) {
    return CloneFate::Keep;
  }
  if uuid::Uuid::parse_str(name).is_err() || !profiles.existing.contains(name) {
    return CloneFate::Remove;
  }
  match read_stamp(dir) {
    Some(stamp) if Path::new(&stamp.source).is_dir() => CloneFate::Keep,
    _ => CloneFate::Remove,
  }
}

/// Rename `dir` to a trash sibling no launch looks at, so deleting it can take
/// its time without a lock. `dir` itself when it is trash already, or when the
/// rename fails.
fn move_to_trash(dir: &Path) -> PathBuf {
  let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
    return dir.to_path_buf();
  };
  if name.starts_with(TRASH_PREFIX) {
    return dir.to_path_buf();
  }
  let n = TRASH_COUNTER.fetch_add(1, Ordering::Relaxed);
  let trash = dir.with_file_name(format!("{TRASH_PREFIX}{name}-{}-{n}", std::process::id()));
  match std::fs::rename(dir, &trash) {
    Ok(()) => trash,
    Err(e) => {
      log::debug!("Per-profile app not moved to trash err=\"{e}\"");
      dir.to_path_buf()
    }
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

/// SHA-256 over `HOOKED_RESOURCES` of a bundle, a missing file included as
/// missing, so a hook appearing or going away changes it too.
fn hooked_resources_digest(bundle: &Path) -> String {
  use sha2::{Digest, Sha256};
  let resources = bundle.join("Contents/Resources");
  let mut hasher = Sha256::new();
  for relative in HOOKED_RESOURCES {
    hasher.update(relative.as_bytes());
    match std::fs::read(resources.join(relative)) {
      Ok(bytes) => {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
      }
      Err(_) => hasher.update(u64::MAX.to_le_bytes()),
    }
  }
  hasher
    .finalize()
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect()
}

fn read_stamp(dir: &Path) -> Option<BundleStamp> {
  let data = std::fs::read(dir.join(STAMP_FILE)).ok()?;
  serde_json::from_slice(&data).ok()
}

fn write_stamp(dir: &Path, stamp: &BundleStamp) -> Result<(), String> {
  let data = serde_json::to_vec_pretty(stamp).map_err(|e| e.to_string())?;
  std::fs::write(dir.join(STAMP_FILE), data).map_err(|e| e.to_string())
}

fn bundle_identifier(profile: &BrowserProfile) -> String {
  // Stable across renames and recolours: macOS keys TCC grants (camera,
  // microphone) by bundle identity, and asking for them again on every edit
  // is worse than a cached Dock icon. `register_bundle` refreshes the icon
  // cache after the bundle is rebuilt with a new name or icon.
  let id = profile.id.to_string().replace('-', "");
  format!("com.owl.camoufox.{id}")
}

fn clone_bundle(source: &Path, destination: &Path) -> Result<(), String> {
  // `-c` is clonefile(2) on APFS: cheap, copy-on-write. A different filesystem
  // falls back to a full copy rather than failing the launch.
  if !copy_tree(source, destination, "-cR") {
    log::warn!(
      "APFS clone unavailable, copying Camoufox.app for a profile source={}",
      source.display()
    );
    // A failed clone can leave part of the bundle behind, and `cp -R` into an
    // existing directory would copy the bundle inside it.
    if std::fs::symlink_metadata(destination).is_ok() {
      std::fs::remove_dir_all(destination).map_err(|e| {
        format!(
          "Failed to clear a partial clone {}: {e}",
          destination.display()
        )
      })?;
    }
    if !copy_tree(source, destination, "-R") {
      return Err("Failed to copy Camoufox.app".to_string());
    }
  }
  if destination.join(EXECUTABLE).is_file() {
    Ok(())
  } else {
    Err(format!(
      "The copied Camoufox.app has no {EXECUTABLE}: {}",
      destination.display()
    ))
  }
}

fn copy_tree(source: &Path, destination: &Path, flags: &str) -> bool {
  Command::new("/bin/cp")
    .arg(flags)
    .arg(source)
    .arg(destination)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// Run a step whose failure changes nothing for the launch, noting why it
/// failed for an agent reading the log.
fn run_best_effort(mut command: Command, what: &str) {
  match command.stdout(Stdio::null()).output() {
    Ok(output) if output.status.success() => {}
    Ok(output) => log::debug!(
      "{what} failed status={} err=\"{}\"",
      output.status,
      String::from_utf8_lossy(&output.stderr).trim()
    ),
    Err(e) => log::debug!("{what} not run err=\"{e}\""),
  }
}

/// Tell LaunchServices the rebuilt bundle's Info.plist and icon changed, so a
/// rename/recolour does not keep the old Dock icon cached. Best effort: the
/// app launches either way.
fn register_bundle(app: &Path) {
  let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
  if !Path::new(lsregister).is_file() {
    return;
  }
  let mut command = Command::new(lsregister);
  command.arg("-f").arg(app);
  run_best_effort(command, "Registering profile app");
}

fn set_bundle_identity(app: &Path, name: &str, bundle_id: &str) -> Result<(), String> {
  let plist = app.join("Contents/Info.plist");
  plutil_set(&plist, "CFBundleDisplayName", name)?;
  plutil_set(&plist, "CFBundleName", name)?;
  plutil_set(&plist, "CFBundleIdentifier", bundle_id)?;
  plutil_set(&plist, "CFBundleIconFile", ICON_FILE)?;
  // `CFBundleIconName` wins over `CFBundleIconFile` when both exist.
  let mut remove = Command::new("/usr/bin/plutil");
  remove.args(["-remove", "CFBundleIconName"]).arg(&plist);
  run_best_effort(remove, "Removing CFBundleIconName");
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
  let mut xattr = Command::new("/usr/bin/xattr");
  xattr.args(["-dr", "com.apple.quarantine"]).arg(app);
  run_best_effort(xattr, "Removing quarantine from the per-profile app");
}

fn sign_bundle(app: &Path) -> Result<(), String> {
  let signed = Command::new("/usr/bin/codesign")
    .args([
      "--force",
      "--deep",
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
    .args(["--force", "--deep", "--sign", "-"])
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
    let executable = app.join(EXECUTABLE);
    std::fs::write(&executable, b"binary").unwrap();
    assert_eq!(bundle_root(&executable), Some(app));
  }

  #[test]
  fn a_changed_hook_or_font_marker_changes_the_resources_digest() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("Camoufox.app");
    let resources = app.join("Contents/Resources");
    std::fs::create_dir_all(resources.join("fonts-shadowed")).unwrap();
    std::fs::write(resources.join("camoufox.cfg"), "// hook v1\n").unwrap();
    let first = hooked_resources_digest(&app);
    assert_eq!(first, hooked_resources_digest(&app), "the digest is stable");

    std::fs::write(resources.join("camoufox.cfg"), "// hook v2\n").unwrap();
    let after_hook = hooked_resources_digest(&app);
    assert_ne!(
      first, after_hook,
      "a changed camoufox.cfg must rebuild clones"
    );

    std::fs::write(resources.join("fonts-shadowed/.owl-checked"), "build 26.1").unwrap();
    assert_ne!(
      after_hook,
      hooked_resources_digest(&app),
      "fonts shadowed again must rebuild clones"
    );
  }

  #[test]
  fn a_stamp_from_before_the_resources_digest_is_not_reused() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
      tmp.path().join(STAMP_FILE),
      r#"{"format":2,"source":"/x","info_modified":1,"executable_modified":1,"name":"n","color":"c","bundle_id":"b"}"#,
    )
    .unwrap();
    assert_eq!(read_stamp(tmp.path()), None);
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
  fn a_renamed_or_recoloured_profile_keeps_its_bundle_identity() {
    let p = profile();
    let first = bundle_identifier(&p);
    assert_eq!(first, bundle_identifier(&p));
    assert!(first.starts_with("com.owl.camoufox."));
  }

  #[test]
  fn the_icon_renderer_produces_a_png_at_the_requested_size() {
    let svg = icon_svg("Client EU", "CE", "#a1b2c3");
    let png = render_icon(&svg, 64).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(png.len() > 100);
  }

  const PROFILE_ID: &str = "0b6f9c1e-3a52-4c1d-9f7e-2d8a4b6c1e90";

  fn states(existing: &[&str], running: &[&str]) -> ProfileStates {
    ProfileStates {
      existing: existing.iter().map(|id| id.to_string()).collect(),
      running: running.iter().map(|id| id.to_string()).collect(),
    }
  }

  fn clone_dir(cache: &Path, name: &str, source: Option<&Path>) -> PathBuf {
    let dir = cache.join(name);
    std::fs::create_dir_all(dir.join(APP_NAME)).unwrap();
    if let Some(source) = source {
      let stamp = BundleStamp {
        format: FORMAT_VERSION,
        source: source.to_string_lossy().to_string(),
        info_modified: 1,
        executable_modified: 1,
        resources: "digest".to_string(),
        name: "n".to_string(),
        color: "#a1b2c3".to_string(),
        bundle_id: "b".to_string(),
      };
      write_stamp(&dir, &stamp).unwrap();
    }
    dir
  }

  #[test]
  fn the_sweep_keeps_a_stamped_clone_of_an_existing_profile_and_build() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Camoufox.app");
    std::fs::create_dir_all(&source).unwrap();
    let dir = clone_dir(tmp.path(), PROFILE_ID, Some(&source));
    assert_eq!(
      clone_fate(&dir, &states(&[PROFILE_ID], &[])),
      CloneFate::Keep
    );
  }

  #[test]
  fn the_sweep_removes_the_clone_of_a_deleted_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Camoufox.app");
    std::fs::create_dir_all(&source).unwrap();
    let dir = clone_dir(tmp.path(), PROFILE_ID, Some(&source));
    assert_eq!(clone_fate(&dir, &states(&[], &[])), CloneFate::Remove);
  }

  #[test]
  fn the_sweep_removes_a_clone_of_a_build_that_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let gone = tmp.path().join("old/Camoufox.app");
    let dir = clone_dir(tmp.path(), PROFILE_ID, Some(&gone));
    assert_eq!(
      clone_fate(&dir, &states(&[PROFILE_ID], &[])),
      CloneFate::Remove
    );
  }

  #[test]
  fn the_sweep_removes_a_clone_without_a_stamp_unless_its_browser_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = clone_dir(tmp.path(), PROFILE_ID, None);
    assert_eq!(
      clone_fate(&dir, &states(&[PROFILE_ID], &[])),
      CloneFate::Remove
    );
    assert_eq!(
      clone_fate(&dir, &states(&[PROFILE_ID], &[PROFILE_ID])),
      CloneFate::Keep,
      "a running browser keeps its clone"
    );
  }

  #[test]
  fn the_sweep_removes_trash_and_names_that_are_not_profile_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Camoufox.app");
    std::fs::create_dir_all(&source).unwrap();
    let trash = clone_dir(
      tmp.path(),
      &format!("{TRASH_PREFIX}{PROFILE_ID}-1-0"),
      Some(&source),
    );
    let stray = clone_dir(tmp.path(), "not-a-profile", Some(&source));
    let all = states(&[PROFILE_ID, "not-a-profile"], &[]);
    assert_eq!(clone_fate(&trash, &all), CloneFate::Remove);
    assert_eq!(clone_fate(&stray, &all), CloneFate::Remove);
  }

  #[test]
  fn a_clone_moved_to_trash_gets_a_unique_sibling_name() {
    let tmp = tempfile::tempdir().unwrap();
    let first = clone_dir(tmp.path(), PROFILE_ID, None);
    let first_trash = move_to_trash(&first);
    let second = clone_dir(tmp.path(), PROFILE_ID, None);
    let second_trash = move_to_trash(&second);
    assert_ne!(first_trash, second_trash);
    for trash in [&first_trash, &second_trash] {
      assert_eq!(trash.parent(), Some(tmp.path()));
      assert!(trash
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(TRASH_PREFIX)));
      assert!(trash.join(APP_NAME).is_dir());
    }
    assert!(!tmp.path().join(PROFILE_ID).exists());
    assert_eq!(move_to_trash(&first_trash), first_trash, "trash stays put");
  }

  #[test]
  fn a_cloned_bundle_must_carry_its_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Camoufox.app");
    std::fs::create_dir_all(source.join("Contents/MacOS")).unwrap();
    std::fs::write(source.join(EXECUTABLE), b"binary").unwrap();
    let clone = tmp.path().join("clone/Owl.app");
    std::fs::create_dir_all(clone.parent().unwrap()).unwrap();
    clone_bundle(&source, &clone).unwrap();
    assert!(clone.join(EXECUTABLE).is_file());
    assert!(!clone.join("Camoufox.app").exists());

    let empty = tmp.path().join("Empty.app");
    std::fs::create_dir_all(empty.join("Contents/MacOS")).unwrap();
    let broken = tmp.path().join("broken/Owl.app");
    std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
    assert!(clone_bundle(&empty, &broken).is_err());
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
      || {},
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
    let verify = Command::new("/usr/bin/codesign")
      .args(["--verify", "--deep", "--strict"])
      .arg(app)
      .status()
      .unwrap();
    assert!(verify.success(), "the clone's signature does not verify");
    let output = Command::new(&executable).arg("--version").output().unwrap();
    assert!(output.status.success(), "{:?}", output);
  }
}
