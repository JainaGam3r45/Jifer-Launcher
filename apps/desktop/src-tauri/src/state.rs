//! Service wiring. Every service owns its own synchronization; there is no global state lock.

use std::sync::{Arc, Mutex, PoisonError};

use jifercraft_auth::{AccountManager, DeviceCode, MicrosoftAuth, RestoreOutcome};
use jifercraft_core::game::{GameLauncher, LaunchObserver, LaunchProgress, LaunchStateEvent, LauncherIdentity};
use jifercraft_core::java::RuntimeManager;
use jifercraft_core::meta::MetadataClient;
use jifercraft_core::process::GameLogLine;
use jifercraft_core::settings::SettingsStore;
use jifercraft_core::{CoreResult, InstanceStore, LauncherPaths};
use jifercraft_net::HttpClient;
use jifercraft_updater::{Schedule, UpdateService};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::logs::LogHub;
use crate::secrets::KeyringStore;
use crate::updater::{LaunchActivity, TauriUpdateDriver};

pub const STATE_EVENT: &str = "launch:state";
pub const PROGRESS_EVENT: &str = "launch:progress";
pub const UPDATE_EVENT: &str = "updates:status";

pub type Accounts = AccountManager<HttpClient, KeyringStore>;
pub type Updates = UpdateService<TauriUpdateDriver, LaunchActivity>;

/// One-off facts the UI should explain after startup (e.g. why the user was signed out).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupNotices {
    pub account: Option<AccountNotice>,
    pub migrated_default_instance: bool,
    pub onedrive_detected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountNotice {
    /// The Electron build's encrypted session cannot be read here; Microsoft users sign in once more.
    LegacySessionDiscarded,
    MicrosoftExpired,
    MicrosoftUnavailable,
}

pub struct AppState {
    pub paths: LauncherPaths,
    pub settings: Arc<SettingsStore>,
    pub instances: Arc<InstanceStore>,
    pub metadata: Arc<MetadataClient<HttpClient>>,
    pub launcher: Arc<GameLauncher<HttpClient>>,
    pub accounts: Arc<Accounts>,
    pub updates: Arc<Updates>,
    pub logs: Arc<LogHub>,
    pub observer: Arc<dyn LaunchObserver>,
    pub notices: Mutex<StartupNotices>,
    pub pending_device_code: Mutex<Option<(DeviceCode, CancellationToken)>>,
}

impl AppState {
    pub fn build(app: &AppHandle, paths: LauncherPaths, logs: Arc<LogHub>) -> CoreResult<Self> {
        let version = app.package_info().version.to_string();
        let http = Arc::new(HttpClient::new(&format!("JiferCraftLauncher/{version}"))?);
        let settings = Arc::new(SettingsStore::new(paths.clone()));
        let instances = Arc::new(InstanceStore::new(paths.instances_dir()));
        let metadata = Arc::new(MetadataClient::new(Arc::clone(&http), &paths.cache_dir())?);
        let runtimes = Arc::new(RuntimeManager::new(
            Arc::clone(&http),
            paths.runtimes_dir(),
            paths.cache_dir(),
        )?);
        let launcher = Arc::new(GameLauncher::new(
            paths.clone(),
            Arc::clone(&http),
            Arc::clone(&metadata),
            runtimes,
            Arc::clone(&instances),
            Arc::clone(&settings),
            LauncherIdentity {
                name: "jifercraft-launcher".into(),
                version,
            },
        ));
        let microsoft =
            crate::config::microsoft_client_id().map(|client_id| MicrosoftAuth::new((*http).clone(), client_id));
        let accounts = Arc::new(AccountManager::new(microsoft, KeyringStore));
        let updates = Arc::new(UpdateService::new(
            TauriUpdateDriver::new(app.clone()),
            LaunchActivity(Arc::clone(&launcher)),
            Schedule::default(),
            jifercraft_core::timestamp::now_iso,
        ));
        let observer: Arc<dyn LaunchObserver> = Arc::new(WindowObserver {
            app: app.clone(),
            logs: Arc::clone(&logs),
        });

        Ok(Self {
            paths,
            settings,
            instances,
            metadata,
            launcher,
            accounts,
            updates,
            logs,
            observer,
            notices: Mutex::default(),
            pending_device_code: Mutex::new(None),
        })
    }

    pub fn notices(&self) -> StartupNotices {
        self.notices.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn record_restore(&self, outcome: RestoreOutcome) {
        let notice = match outcome {
            RestoreOutcome::MicrosoftExpired => Some(AccountNotice::MicrosoftExpired),
            RestoreOutcome::MicrosoftUnavailable => Some(AccountNotice::MicrosoftUnavailable),
            _ => None,
        };
        if notice.is_some() {
            self.notices.lock().unwrap_or_else(PoisonError::into_inner).account = notice;
        }
    }
}

/// Forwards launch events to the window; game output goes through the batched log hub.
struct WindowObserver {
    app: AppHandle,
    logs: Arc<LogHub>,
}

impl LaunchObserver for WindowObserver {
    fn state(&self, event: &LaunchStateEvent) {
        let _ = self.app.emit(STATE_EVENT, event);
    }

    fn progress(&self, event: &LaunchProgress) {
        let _ = self.app.emit(PROGRESS_EVENT, event);
    }

    fn log(&self, line: GameLogLine) {
        self.logs.push_game(line);
    }
}
