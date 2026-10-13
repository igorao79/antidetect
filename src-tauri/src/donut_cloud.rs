//! What is left of Donut's cloud in this fork: nothing the app reaches for on
//! its own.
//!
//! The fork has no Donut account, so the interface leaves out signing in, sync
//! and the Donut licence (`src/lib/cloud-only-pages.ts`). The backend keeps
//! upstream's cloud code for merges; this module answers the one question the
//! rest of the backend needs: whether to treat a cloud-bound feature as there.

use crate::downloaded_browsers_registry::DownloadedBrowsersRegistry;

/// Wayfern takes its fingerprints, its launch token and its downloads from a
/// Donut account. Without one it is offered only where a build is already
/// installed, which also keeps the app from asking Donut's server for the
/// current Wayfern version at every start.
pub fn wayfern_offered() -> bool {
  !DownloadedBrowsersRegistry::instance()
    .get_downloaded_versions("wayfern")
    .is_empty()
}
