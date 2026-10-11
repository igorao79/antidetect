//! A video file standing in for the camera of a CloakBrowser profile.
//!
//! Chromium reads its camera from a file with `--use-file-for-fake-video-capture`,
//! which only takes effect together with `--use-fake-device-for-media-stream`.
//! That second switch replaces the microphone as well, with a device that
//! beeps, so a silent recording is supplied for it.
//!
//! What a site gets is the video, on a loop. What it can still tell, if it
//! lists devices after being granted access: the camera is named after the
//! file and the microphone is called "Fake Audio Input". The free build has no
//! switch for those names.

use std::path::{Path, PathBuf};

/// The camera's label is this file's name, as the browser is started in its
/// directory and given the name alone.
const CAMERA_FILE: &str = "Integrated Camera.mjpeg";
const SILENCE_FILE: &str = "silence.wav";
/// Motion JPEG runs to megabytes per second; a longer clip only loops later.
const MAX_SECONDS: &str = "60";

pub fn camera_dir(profiles_dir: &Path, profile_id: &str) -> PathBuf {
  profiles_dir.join(profile_id).join("camera")
}

fn coded(code: &str, detail: impl std::fmt::Display) -> String {
  crate::backend_error_with_detail(code, detail)
}

/// `ffmpeg`, looked for beyond `PATH`: an app started from the Finder does not
/// inherit the shell's.
fn find_ffmpeg() -> Option<PathBuf> {
  let from_path = std::env::var_os("PATH")
    .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
    .unwrap_or_default();
  from_path
    .into_iter()
    .chain(
      ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .iter()
        .map(PathBuf::from),
    )
    .map(|dir| dir.join("ffmpeg"))
    .find(|candidate| candidate.is_file())
}

/// One second of silence: 48 kHz, mono, 16-bit PCM.
fn silent_wav() -> Vec<u8> {
  const RATE: u32 = 48_000;
  let data_len = RATE * 2;
  let mut wav = Vec::with_capacity(44 + data_len as usize);
  wav.extend_from_slice(b"RIFF");
  wav.extend_from_slice(&(36 + data_len).to_le_bytes());
  wav.extend_from_slice(b"WAVEfmt ");
  wav.extend_from_slice(&16u32.to_le_bytes());
  wav.extend_from_slice(&1u16.to_le_bytes());
  wav.extend_from_slice(&1u16.to_le_bytes());
  wav.extend_from_slice(&RATE.to_le_bytes());
  wav.extend_from_slice(&(RATE * 2).to_le_bytes());
  wav.extend_from_slice(&2u16.to_le_bytes());
  wav.extend_from_slice(&16u16.to_le_bytes());
  wav.extend_from_slice(b"data");
  wav.extend_from_slice(&data_len.to_le_bytes());
  wav.resize(44 + data_len as usize, 0);
  wav
}

/// Make `source` the camera video kept in `dir`. A Motion JPEG file is taken
/// as it is; anything else is converted with ffmpeg.
pub async fn install(dir: &Path, source: &Path) -> Result<(), String> {
  if !source.is_file() {
    return Err(crate::backend_error("CAMERA_VIDEO_NOT_FOUND"));
  }
  std::fs::create_dir_all(dir).map_err(|e| coded("INTERNAL_ERROR", e))?;
  let target = dir.join(CAMERA_FILE);

  let is_mjpeg = source
    .extension()
    .is_some_and(|ext| ext.eq_ignore_ascii_case("mjpeg"));
  if is_mjpeg {
    std::fs::copy(source, &target).map_err(|e| coded("INTERNAL_ERROR", e))?;
  } else {
    let ffmpeg = find_ffmpeg().ok_or_else(|| crate::backend_error("CAMERA_VIDEO_NEEDS_FFMPEG"))?;
    let output = tokio::process::Command::new(ffmpeg)
      .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
      .arg(source)
      .args([
        "-an",
        "-t",
        MAX_SECONDS,
        "-vf",
        "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,fps=30",
        "-c:v",
        "mjpeg",
        "-pix_fmt",
        "yuvj420p",
        "-q:v",
        "4",
        "-f",
        "mjpeg",
      ])
      .arg(&target)
      .output()
      .await
      .map_err(|e| coded("CAMERA_VIDEO_CONVERSION_FAILED", e))?;
    if !output.status.success() {
      let _ = std::fs::remove_file(&target);
      let reason = String::from_utf8_lossy(&output.stderr);
      let reason = reason.lines().last().unwrap_or("ffmpeg failed").to_string();
      return Err(coded("CAMERA_VIDEO_CONVERSION_FAILED", reason));
    }
  }
  std::fs::write(dir.join(SILENCE_FILE), silent_wav()).map_err(|e| coded("INTERNAL_ERROR", e))
}

pub fn remove(dir: &Path) {
  if dir.exists() {
    if let Err(e) = std::fs::remove_dir_all(dir) {
      log::warn!("Could not remove camera video {}: {e}", dir.display());
    }
  }
}

/// The switches that put the video in `dir` in place of the camera, for a
/// browser started with `dir` as its working directory. `None` when no video
/// is installed there.
pub fn launch_args(dir: &Path) -> Option<Vec<String>> {
  dir.join(CAMERA_FILE).is_file().then(|| {
    vec![
      "--use-fake-device-for-media-stream".to_string(),
      format!("--use-file-for-fake-video-capture={CAMERA_FILE}"),
      format!("--use-file-for-fake-audio-capture={SILENCE_FILE}"),
    ]
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_silent_recording_is_a_well_formed_wav() {
    let wav = silent_wav();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(
      u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
      wav.len() - 8
    );
    assert_eq!(
      u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
      wav.len() - 44
    );
    assert!(wav[44..].iter().all(|byte| *byte == 0));
  }

  #[tokio::test]
  async fn a_motion_jpeg_file_is_installed_as_it_is_and_removed_again() {
    let temp = tempfile::TempDir::new().unwrap();
    let source = temp.path().join("clip.MJPEG");
    std::fs::write(&source, b"\xff\xd8frame\xff\xd9").unwrap();
    let dir = camera_dir(temp.path(), "profile-1");

    assert_eq!(launch_args(&dir), None);
    install(&dir, &source).await.unwrap();
    assert_eq!(
      std::fs::read(dir.join(CAMERA_FILE)).unwrap(),
      b"\xff\xd8frame\xff\xd9"
    );
    let args = launch_args(&dir).unwrap();
    assert!(args.contains(&"--use-fake-device-for-media-stream".to_string()));
    assert!(args.contains(&format!("--use-file-for-fake-video-capture={CAMERA_FILE}")));
    // Names only: the browser resolves them against its working directory.
    assert!(args.iter().all(|arg| !arg.contains('/')));

    remove(&dir);
    assert_eq!(launch_args(&dir), None);
  }

  #[tokio::test]
  async fn a_missing_file_is_reported_by_code() {
    let temp = tempfile::TempDir::new().unwrap();
    let error = install(&temp.path().join("camera"), &temp.path().join("nope.mp4"))
      .await
      .unwrap_err();
    assert!(error.contains("CAMERA_VIDEO_NOT_FOUND"), "{error}");
  }
}
