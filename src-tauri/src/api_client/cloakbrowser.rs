//! CloakBrowser (anti-detect Chromium) builds from cloakbrowser.dev.
//!
//! Two tiers. The free build is one pinned version per platform. With a
//! CloakBrowser Pro licence key (`CLOAKBROWSER_LICENSE_KEY`, or
//! `~/.cloakbrowser/license.key`, where the binary itself looks) the current
//! Pro build is offered as well: its version is asked of CloakHQ's server and
//! its archive is downloaded with the key.
//!
//! Every release publishes `SHA256SUMS` and a detached Ed25519 signature
//! `SHA256SUMS.sig`. The signature is checked against CloakHQ's published key
//! before any digest in the manifest is trusted, so the download origin alone
//! cannot vouch for a tampered archive.

use super::ApiClient;

const DOWNLOAD_BASE_URL: &str = "https://cloakbrowser.dev";

/// CloakHQ's binary signing keys (base64 of the raw 32-byte Ed25519 key), as
/// pinned by their own wrapper. More than one entry allows key rotation.
const SIGNING_PUBKEYS: &[&str] = &["MKFKwIhUcKWq5xTuNA0Ovg99njcDEcEJvmWYYhApvaU="];

/// Sent with every Pro request so the server answers for this machine's build.
const PLATFORM_HEADER: &str = "X-Platform";

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The licence server's answer about one key.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct CloakBrowserLicenseVerdict {
  pub valid: bool,
  #[serde(default)]
  pub plan: Option<String>,
  #[serde(default)]
  pub expires: Option<String>,
}

/// A licence key as it is used: trimmed, and nothing at all when blank.
fn clean_key(raw: &str) -> Option<String> {
  let key = raw.trim();
  (!key.is_empty()).then(|| key.to_string())
}

impl ApiClient {
  /// Where the Pro binary reads its key from when the environment has none.
  pub fn cloakbrowser_license_file() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".cloakbrowser").join("license.key"))
  }

  /// The CloakBrowser Pro licence key in effect, if there is one.
  pub fn cloakbrowser_license_key() -> Option<String> {
    std::env::var("CLOAKBROWSER_LICENSE_KEY")
      .ok()
      .and_then(|key| clean_key(&key))
      .or_else(|| {
        let file = Self::cloakbrowser_license_file()?;
        clean_key(&std::fs::read_to_string(file).ok()?)
      })
  }

  /// What CloakHQ's licence server says about `key`.
  pub async fn validate_cloakbrowser_license(
    &self,
    key: &str,
  ) -> Result<CloakBrowserLicenseVerdict, BoxError> {
    let response = self
      .client
      .post(format!("{DOWNLOAD_BASE_URL}/api/license/validate"))
      .header("User-Agent", "antidetect-browser")
      .json(&serde_json::json!({ "license_key": key }))
      .timeout(std::time::Duration::from_secs(15))
      .send()
      .await?;
    if !response.status().is_success() {
      return Err(
        format!(
          "CloakBrowser licence check: HTTP {}",
          response.status().as_u16()
        )
        .into(),
      );
    }
    Ok(response.json().await?)
  }

  /// Whether `version` is a Pro build: any CloakBrowser version other than
  /// this platform's free one.
  pub fn cloakbrowser_is_pro(version: &str) -> bool {
    Self::cloakbrowser_version().is_some_and(|free| free != version)
  }

  /// The current Pro build for this machine, as CloakHQ's server names it.
  pub async fn fetch_cloakbrowser_pro_version(&self) -> Result<String, BoxError> {
    let (tag, _) =
      Self::cloakbrowser_platform().ok_or("CloakBrowser has no build for this platform")?;
    let response = self
      .client
      .get(format!("{DOWNLOAD_BASE_URL}/api/download/version"))
      .header("User-Agent", "antidetect-browser")
      .header(PLATFORM_HEADER, tag)
      .timeout(std::time::Duration::from_secs(15))
      .send()
      .await?;
    if !response.status().is_success() {
      return Err(
        format!(
          "CloakBrowser Pro version: HTTP {}",
          response.status().as_u16()
        )
        .into(),
      );
    }
    let body: serde_json::Value = response.json().await?;
    body["version"]
      .as_str()
      .filter(|version| !version.is_empty())
      .map(str::to_string)
      .ok_or_else(|| "CloakBrowser Pro version: the server named none".into())
  }

  /// Every build this machine can install, newest first: the Pro build when a
  /// key is set and the server can be asked, then the free one.
  pub async fn cloakbrowser_versions(&self) -> Vec<String> {
    let mut versions = Vec::new();
    if Self::cloakbrowser_license_key().is_some() {
      match self.fetch_cloakbrowser_pro_version().await {
        Ok(version) => versions.push(version),
        Err(e) => log::warn!("Could not learn the CloakBrowser Pro version: {e}"),
      }
    }
    if let Some(free) = Self::cloakbrowser_version() {
      if !versions.iter().any(|version| version == free) {
        versions.push(free.to_string());
      }
    }
    versions
  }

  /// The headers a download of `url` needs beyond the usual ones: the licence
  /// key and platform for a Pro archive, nothing for anything else.
  pub fn cloakbrowser_download_headers(url: &str) -> Vec<(&'static str, String)> {
    if !url.starts_with(&format!("{DOWNLOAD_BASE_URL}/api/download/")) {
      return Vec::new();
    }
    let mut headers = Vec::new();
    if let Some(key) = Self::cloakbrowser_license_key() {
      headers.push(("Authorization", format!("Bearer {key}")));
    }
    if let Some((tag, _)) = Self::cloakbrowser_platform() {
      headers.push((PLATFORM_HEADER, tag.to_string()));
    }
    headers
  }

  /// `(platform tag, free Chromium build)` for this machine.
  fn cloakbrowser_platform() -> Option<(&'static str, &'static str)> {
    let (os, arch) = Self::get_platform_info();
    match (os.as_str(), arch.as_str()) {
      ("macos", "arm64") => Some(("darwin-arm64", "145.0.7632.109.2")),
      ("macos", "x64") => Some(("darwin-x64", "145.0.7632.109.2")),
      ("linux", "x64") => Some(("linux-x64", "146.0.7680.177.5")),
      ("linux", "arm64") => Some(("linux-arm64", "146.0.7680.177.3")),
      ("windows", "x64") => Some(("windows-x64", "146.0.7680.177.5")),
      _ => None,
    }
  }

  /// The CloakBrowser build available to this machine, if any.
  pub fn cloakbrowser_version() -> Option<&'static str> {
    Self::cloakbrowser_platform().map(|(_, version)| version)
  }

  /// The release archive's file name, e.g. `cloakbrowser-darwin-arm64.tar.gz`.
  pub fn cloakbrowser_archive_name() -> Option<String> {
    let (tag, _) = Self::cloakbrowser_platform()?;
    let ext = if tag.starts_with("windows") {
      "zip"
    } else {
      "tar.gz"
    };
    Some(format!("cloakbrowser-{tag}.{ext}"))
  }

  pub fn cloakbrowser_download_url(version: &str) -> Option<String> {
    let archive = Self::cloakbrowser_archive_name()?;
    Some(if Self::cloakbrowser_is_pro(version) {
      // The server picks the archive from the platform header.
      format!("{DOWNLOAD_BASE_URL}/api/download/{version}")
    } else {
      format!("{DOWNLOAD_BASE_URL}/chromium-v{version}/{archive}")
    })
  }

  /// The SHA-256 the signed manifest of `version` lists for this machine's
  /// archive.
  pub async fn cloakbrowser_archive_digest(&self, version: &str) -> Result<String, BoxError> {
    let archive =
      Self::cloakbrowser_archive_name().ok_or("CloakBrowser has no build for this platform")?;
    // Pro releases are signed with the same keys and published beside the
    // free ones.
    let base = if Self::cloakbrowser_is_pro(version) {
      format!("{DOWNLOAD_BASE_URL}/releases/pro/chromium-v{version}")
    } else {
      format!("{DOWNLOAD_BASE_URL}/chromium-v{version}")
    };
    let manifest = self.fetch_bytes(&format!("{base}/SHA256SUMS")).await?;
    let signature = self.fetch_bytes(&format!("{base}/SHA256SUMS.sig")).await?;
    digest_from_signed_manifest(&manifest, &signature, version, &archive, SIGNING_PUBKEYS)
  }

  async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>, BoxError> {
    let response = self
      .client
      .get(url)
      .header("User-Agent", "antidetect-browser")
      .send()
      .await?;
    if !response.status().is_success() {
      return Err(format!("{url}: HTTP {}", response.status().as_u16()).into());
    }
    Ok(response.bytes().await?.to_vec())
  }
}

/// Verify `signature` over `manifest` with one of `pubkeys`, check the manifest
/// is for `version`, and return the digest it lists for `archive`.
///
/// The version line matters: the signature proves CloakHQ made this manifest,
/// not that it is the one asked for, so without it a mirror could answer with
/// a genuinely signed manifest of an older, vulnerable build.
fn digest_from_signed_manifest(
  manifest: &[u8],
  signature: &[u8],
  version: &str,
  archive: &str,
  pubkeys: &[&str],
) -> Result<String, BoxError> {
  use base64::{engine::general_purpose::STANDARD, Engine as _};

  let signature = STANDARD
    .decode(String::from_utf8_lossy(signature).trim())
    .map_err(|e| format!("CloakBrowser signature is not valid base64: {e}"))?;
  let signed = pubkeys.iter().any(|key| {
    STANDARD.decode(key).is_ok_and(|key| {
      ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(manifest, &signature)
        .is_ok()
    })
  });
  if !signed {
    return Err("CloakBrowser SHA256SUMS signature does not verify".into());
  }

  let text = std::str::from_utf8(manifest)?;
  let declared = text
    .lines()
    .find_map(|line| line.trim().strip_prefix("version="))
    .map(str::trim);
  if declared != Some(version) {
    return Err(
      format!(
        "CloakBrowser manifest is for version {}, not {version}",
        declared.unwrap_or("(none)")
      )
      .into(),
    );
  }

  crate::checksum::find_checksum_for_file(text, archive)
    .ok_or_else(|| format!("CloakBrowser manifest has no entry for {archive}").into())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_refused_key_is_read_as_invalid_with_no_expiry() {
    let verdict: CloakBrowserLicenseVerdict =
      serde_json::from_str(r#"{"valid":false,"plan":"unknown","expires":null}"#).unwrap();
    assert!(!verdict.valid);
    assert_eq!(verdict.expires, None);
    let verdict: CloakBrowserLicenseVerdict = serde_json::from_str(
      r#"{"valid":true,"plan":"trial","expires":"2026-10-14T03:04:12+00:00"}"#,
    )
    .unwrap();
    assert!(verdict.valid);
    assert_eq!(verdict.plan.as_deref(), Some("trial"));
  }

  #[test]
  fn a_key_is_trimmed_and_a_blank_one_is_no_key() {
    assert_eq!(clean_key("  cb_abc\n"), Some("cb_abc".to_string()));
    assert_eq!(clean_key(" \n"), None);
  }

  #[test]
  fn any_version_but_the_pinned_free_one_is_a_pro_build() {
    let Some(free) = ApiClient::cloakbrowser_version() else {
      return;
    };
    assert!(!ApiClient::cloakbrowser_is_pro(free));
    assert!(ApiClient::cloakbrowser_is_pro("154.0.8037.57.1"));
  }

  #[test]
  fn a_pro_build_is_downloaded_from_the_licensed_endpoint_and_verified_beside_it() {
    let Some(free) = ApiClient::cloakbrowser_version() else {
      return;
    };
    let free_url = ApiClient::cloakbrowser_download_url(free).unwrap();
    assert!(free_url.starts_with(&format!("{DOWNLOAD_BASE_URL}/chromium-v{free}/")));
    // The free archive is public: nothing identifying goes with the request.
    assert!(ApiClient::cloakbrowser_download_headers(&free_url).is_empty());

    let pro_url = ApiClient::cloakbrowser_download_url("154.0.8037.57.1").unwrap();
    assert_eq!(
      pro_url,
      format!("{DOWNLOAD_BASE_URL}/api/download/154.0.8037.57.1")
    );
    let headers = ApiClient::cloakbrowser_download_headers(&pro_url);
    assert!(headers.iter().any(|(name, _)| *name == PLATFORM_HEADER));
    // Another host never gets the key, whatever its path looks like.
    assert!(ApiClient::cloakbrowser_download_headers(
      "https://example.com/api/download/154.0.8037.57.1"
    )
    .is_empty());
  }
  use base64::{engine::general_purpose::STANDARD, Engine as _};
  use ring::signature::KeyPair;

  const DIGEST: &str = "505582aa1bd3971c577f70e0cbbe016431702bdb693529abfd943b5bd9120c1c";

  fn signed(manifest: &str) -> (Vec<u8>, String) {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let signature = STANDARD.encode(pair.sign(manifest.as_bytes()).as_ref());
    (
      format!("{signature}\n").into_bytes(),
      STANDARD.encode(pair.public_key().as_ref()),
    )
  }

  fn manifest() -> String {
    format!("version=145.0.7632.109.2\n{DIGEST}  cloakbrowser-darwin-arm64.tar.gz\n")
  }

  /// The published manifest and signature of the free macOS build: proves the
  /// pinned key and the wire format, not just our own round trip.
  #[test]
  fn the_published_v145_manifest_verifies_against_the_pinned_key() {
    let manifest = "version=145.0.7632.109.2\n\
caf71fa5b85b3fc33253199b52ac58b96b959f00987863b4eef684ecc3cc6077  cloakbrowser-linux-x64.tar.gz\n\
505582aa1bd3971c577f70e0cbbe016431702bdb693529abfd943b5bd9120c1c  cloakbrowser-darwin-arm64.tar.gz\n\
a0a061b1945db55b3bbe95986a82b67803311c0fedabb3946e7ab0172273daac  cloakbrowser-darwin-x64.tar.gz\n\
ee6e984d077f5d71bf7aeef51e58cb596ee5ab2296ca09e2210a37f88e5485ef  cloakbrowser-windows-x64.zip\n";
    let signature =
      "2Ijmw9f34S7cGc2p6Lp5K4H8cY9nap5td08cvnQykoESQQ3yA9AczrhlloRloKIjR56kqouJ2hVpfRv1uMQeCA==";
    let digest = digest_from_signed_manifest(
      manifest.as_bytes(),
      signature.as_bytes(),
      "145.0.7632.109.2",
      "cloakbrowser-darwin-arm64.tar.gz",
      SIGNING_PUBKEYS,
    )
    .unwrap();
    assert_eq!(digest, DIGEST);
  }

  #[test]
  fn a_signed_manifest_yields_the_archive_digest() {
    let manifest = manifest();
    let (signature, key) = signed(&manifest);
    let digest = digest_from_signed_manifest(
      manifest.as_bytes(),
      &signature,
      "145.0.7632.109.2",
      "cloakbrowser-darwin-arm64.tar.gz",
      &[&key],
    )
    .unwrap();
    assert_eq!(digest, DIGEST);
  }

  #[test]
  fn a_tampered_manifest_is_rejected() {
    let manifest = manifest();
    let (signature, key) = signed(&manifest);
    let tampered = manifest.replace("505582", "000000");
    assert!(digest_from_signed_manifest(
      tampered.as_bytes(),
      &signature,
      "145.0.7632.109.2",
      "cloakbrowser-darwin-arm64.tar.gz",
      &[&key],
    )
    .is_err());
  }

  #[test]
  fn a_manifest_signed_by_another_key_is_rejected() {
    let manifest = manifest();
    let (signature, _) = signed(&manifest);
    assert!(digest_from_signed_manifest(
      manifest.as_bytes(),
      &signature,
      "145.0.7632.109.2",
      "cloakbrowser-darwin-arm64.tar.gz",
      SIGNING_PUBKEYS,
    )
    .is_err());
  }

  #[test]
  fn a_manifest_for_another_version_is_rejected() {
    let manifest = manifest();
    let (signature, key) = signed(&manifest);
    let error = digest_from_signed_manifest(
      manifest.as_bytes(),
      &signature,
      "146.0.7680.177.5",
      "cloakbrowser-darwin-arm64.tar.gz",
      &[&key],
    )
    .unwrap_err();
    assert!(error.to_string().contains("146.0.7680.177.5"), "{error}");
  }

  #[test]
  fn an_archive_missing_from_the_manifest_is_rejected() {
    let manifest = manifest();
    let (signature, key) = signed(&manifest);
    assert!(digest_from_signed_manifest(
      manifest.as_bytes(),
      &signature,
      "145.0.7632.109.2",
      "cloakbrowser-windows-x64.zip",
      &[&key],
    )
    .is_err());
  }
}
