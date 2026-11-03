//! `UpdateDriver` over `tauri-plugin-updater`: minisign-verified packages from GitHub Releases.

use std::sync::{Arc, Mutex, PoisonError};

use jifercraft_core::game::GameLauncher;
use jifercraft_net::HttpClient;
use jifercraft_updater::{ChunkFn, GameActivity, Release, UpdateDriver};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};
use time::format_description::well_known::Rfc3339;

pub struct TauriUpdateDriver {
    app: AppHandle,
    offered: Mutex<Option<Update>>,
    downloaded: Mutex<Option<(Update, Vec<u8>)>>,
}

impl TauriUpdateDriver {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            offered: Mutex::new(None),
            downloaded: Mutex::new(None),
        }
    }
}

impl UpdateDriver for TauriUpdateDriver {
    fn supported(&self) -> bool {
        // Development builds are not installed through NSIS and have nothing to update.
        !cfg!(debug_assertions)
    }

    fn current_version(&self) -> String {
        self.app.package_info().version.to_string()
    }

    async fn check(&self) -> Result<Option<Release>, String> {
        let update = self
            .app
            .updater()
            .map_err(|error| error.to_string())?
            .check()
            .await
            .map_err(|error| error.to_string())?;
        let release = update.as_ref().map(|update| Release {
            version: update.version.clone(),
            date: update.date.and_then(|date| date.format(&Rfc3339).ok()),
            notes: update.body.clone(),
        });
        *self.offered.lock().unwrap_or_else(PoisonError::into_inner) = update;
        Ok(release)
    }

    async fn download(&self, on_chunk: ChunkFn) -> Result<(), String> {
        let update = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| "no update has been offered".to_owned())?;
        let mut received = 0_u64;
        let bytes = update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    on_chunk(received, total);
                },
                || {},
            )
            .await
            .map_err(|error| error.to_string())?;
        *self.downloaded.lock().unwrap_or_else(PoisonError::into_inner) = Some((update, bytes));
        Ok(())
    }

    fn install(&self) -> Result<(), String> {
        let (update, bytes) = self
            .downloaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| "no update has been downloaded".to_owned())?;
        // On Windows the NSIS installer takes over and exits this process; elsewhere restart explicitly.
        update.install(bytes).map_err(|error| error.to_string())?;
        self.app.restart()
    }
}

/// "Is Minecraft running?" for the updater's install guard.
pub struct LaunchActivity(pub Arc<GameLauncher<HttpClient>>);

impl GameActivity for LaunchActivity {
    fn is_running(&self) -> bool {
        self.0.active_instance().is_some()
    }
}
