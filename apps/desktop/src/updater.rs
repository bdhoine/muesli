//! The `Updater` adapter: `tauri-plugin-updater`, the same on macOS and Windows. It reads
//! `latest.json` of the latest GitHub release (the endpoint, the public key and the Windows install
//! mode are in `tauri.release.conf.json`), downloads the update and verifies its signature against
//! that key. The engine decides when; this only does what it is told.
//!
//! macOS: installing replaces the app bundle; the new version starts with `restart`.
//! Windows: installing runs the per-user NSIS installer in passive mode, which closes Muesli and,
//! for Restart, starts it again.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use muesli_core::ports::{OnProgress, Release, UpdateError, Updater};
use muesli_core::update::CONNECT_TIMEOUT_SECONDS;
use tauri::AppHandle;
use tauri_plugin_updater::{Error, Update, UpdaterExt};

#[derive(Default)]
pub struct TauriUpdater {
    handle: OnceLock<AppHandle>,
    /// What the last check found.
    found: Mutex<Option<Update>>,
    /// Downloaded and verified, waiting to be installed.
    downloaded: Mutex<Option<(Update, Vec<u8>)>>,
}

fn network(e: Error) -> UpdateError {
    match e {
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) => UpdateError::BadSignature,
        other => UpdateError::Network(other.to_string()),
    }
}

impl TauriUpdater {
    pub fn attach(&self, handle: &AppHandle) {
        let _ = self.handle.set(handle.clone());
    }

    fn handle(&self) -> Result<&AppHandle, UpdateError> {
        self.handle.get().ok_or_else(|| UpdateError::Network("Muesli is still starting.".into()))
    }

    fn take_download(&self) -> Result<(Update, Vec<u8>), UpdateError> {
        self.downloaded.lock().unwrap().take().ok_or_else(|| UpdateError::Install("No update was downloaded.".into()))
    }
}

impl Updater for TauriUpdater {
    fn current_version(&self) -> String {
        muesli_core::VERSION.to_string()
    }

    fn check(&self) -> Result<Option<Release>, UpdateError> {
        let updater = self
            .handle()?
            .updater_builder()
            .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
            .build()
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        let found = match tauri::async_runtime::block_on(updater.check()) {
            Ok(found) => found,
            // No release yet, or a release without latest.json: up to date.
            Err(Error::ReleaseNotFound) => None,
            Err(e) => return Err(network(e)),
        };
        // The plugin offers only newer versions; the engine checks that again.
        let release = found.as_ref().map(|u| Release { version: u.version.clone(), notes: u.body.clone() });
        *self.found.lock().unwrap() = found;
        Ok(release)
    }

    fn download(&self, release: &Release, on_progress: OnProgress) -> Result<(), UpdateError> {
        let update = self
            .found
            .lock()
            .unwrap()
            .clone()
            .filter(|u| u.version == release.version)
            .ok_or_else(|| UpdateError::Network("This release was not found by the last check.".into()))?;
        let mut received = 0u64;
        // `download` verifies the signature; a download that does not match is dropped here.
        let bytes = tauri::async_runtime::block_on(update.download(
            |chunk, total| {
                received += chunk as u64;
                on_progress(received, total);
            },
            || {},
        ))
        .map_err(network)?;
        *self.downloaded.lock().unwrap() = Some((update, bytes));
        Ok(())
    }

    fn install_and_restart(&self) -> Result<(), UpdateError> {
        let handle = self.handle()?.clone();
        let (update, bytes) = self.take_download()?;
        // Windows: the installer closes Muesli and starts the new version itself.
        update.install(bytes).map_err(|e| UpdateError::Install(e.to_string()))?;
        handle.restart()
    }

    fn install_on_quit(&self) -> Result<(), UpdateError> {
        let (update, bytes) = self.take_download()?;
        update.restart_after_install(false).install(bytes).map_err(|e| UpdateError::Install(e.to_string()))
    }
}
