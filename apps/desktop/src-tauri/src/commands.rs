//! IPC surface. Inputs are validated by the domain types they deserialize into; every failure carries a
//! stable error code.

// Tauri injects command arguments (`State`, `AppHandle`, payloads) by value; borrowing is not an option.
#![allow(clippy::needless_pass_by_value)]

use std::sync::{Arc, PoisonError};

use jifercraft_auth::{DeviceCode, Session};
use jifercraft_core::game::{LaunchState, LaunchStateEvent};
use jifercraft_core::instance::{CloneInstanceInput, CreateInstanceInput, UpdateInstanceInput};
use jifercraft_core::launch::LaunchAuth;
use jifercraft_core::meta::VersionEntry;
use jifercraft_core::settings::{LauncherSettings, MAX_OFFLINE_ACCOUNTS, SettingsPatch};
use jifercraft_core::{CoreResult, Instance, InstanceId};
use jifercraft_updater::{CheckOrigin, UpdateStatus};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

use crate::error::{IpcError, IpcResult};
use crate::logs::LogEntry;
use crate::state::{AppState, StartupNotices};

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> CoreResult<T> + Send + 'static) -> IpcResult<T> {
    Ok(tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| IpcError::new("task_failed", error.to_string()))??)
}

fn instance_id(raw: &str) -> IpcResult<InstanceId> {
    Ok(InstanceId::parse(raw)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    version: String,
    microsoft_available: bool,
    notices: StartupNotices,
    data_dir: String,
}

#[tauri::command]
pub fn app_bootstrap(app: AppHandle, state: State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        version: app.package_info().version.to_string(),
        microsoft_available: state.accounts.microsoft_available(),
        notices: state.notices(),
        data_dir: state.paths.root().to_string_lossy().into_owned(),
    }
}

/// Opens web links in the user's browser. Only `https` is allowed, so the UI cannot be tricked into
/// launching local files or custom protocol handlers.
#[tauri::command]
pub fn app_open_external(app: AppHandle, url: String) -> IpcResult<()> {
    let parsed = url::Url::parse(&url).map_err(|_| IpcError::new("invalid_url", "The link is not a valid URL."))?;
    if parsed.scheme() != "https" {
        return Err(IpcError::new("invalid_url", "Only https links can be opened."));
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|error| IpcError::new("open_failed", error.to_string()))
}

#[tauri::command]
pub fn app_open_data_folder(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    let path = state.paths.root().to_string_lossy().into_owned();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|error| IpcError::new("open_failed", error.to_string()))
}

const MAX_EXPORT_BYTES: usize = 16 * 1024 * 1024;

/// Saves the console text under `logs/exports/` and reveals it in the file manager. The file name is
/// generated here so the UI cannot choose where on disk it writes.
#[tauri::command]
pub async fn logs_export(app: AppHandle, state: State<'_, AppState>, contents: String) -> IpcResult<String> {
    if contents.len() > MAX_EXPORT_BYTES {
        return Err(IpcError::new(
            "export_too_large",
            "The console is too large to export at once.",
        ));
    }
    let name = format!(
        "console-{}.log",
        jifercraft_core::timestamp::now_iso().replace(':', "-")
    );
    let path = state.paths.logs_dir().join("exports").join(name);
    let target = path.clone();
    blocking(move || jifercraft_core::store::write_atomic(&target, contents.as_bytes())).await?;
    if let Err(error) = app.opener().reveal_item_in_dir(&path) {
        tracing::warn!(%error, "could not reveal the exported log");
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn logs_get_recent(state: State<'_, AppState>, limit: Option<usize>) -> Vec<LogEntry> {
    state.logs.recent(limit.unwrap_or(500).min(2000))
}

// ── settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> IpcResult<LauncherSettings> {
    let settings = Arc::clone(&state.settings);
    blocking(move || settings.get()).await
}

#[tauri::command]
pub async fn settings_update(state: State<'_, AppState>, patch: SettingsPatch) -> IpcResult<LauncherSettings> {
    let settings = Arc::clone(&state.settings);
    blocking(move || settings.update(patch)).await
}

// ── versions ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn versions_list(state: State<'_, AppState>) -> IpcResult<Vec<VersionEntry>> {
    Ok(state.metadata.manifest().await?.versions)
}

// ── instances ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn instances_list(state: State<'_, AppState>) -> IpcResult<Vec<Instance>> {
    let instances = Arc::clone(&state.instances);
    blocking(move || instances.list()).await
}

#[tauri::command]
pub async fn instances_get(state: State<'_, AppState>, id: String) -> IpcResult<Instance> {
    let (instances, id) = (Arc::clone(&state.instances), instance_id(&id)?);
    blocking(move || instances.get(&id)).await
}

#[tauri::command]
pub async fn instances_create(state: State<'_, AppState>, input: CreateInstanceInput) -> IpcResult<Instance> {
    let instances = Arc::clone(&state.instances);
    blocking(move || instances.create(input)).await
}

#[tauri::command]
pub async fn instances_clone(state: State<'_, AppState>, input: CloneInstanceInput) -> IpcResult<Instance> {
    let instances = Arc::clone(&state.instances);
    blocking(move || instances.clone_instance(&input)).await
}

#[tauri::command]
pub async fn instances_update(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateInstanceInput,
) -> IpcResult<Instance> {
    let (instances, id) = (Arc::clone(&state.instances), instance_id(&id)?);
    blocking(move || instances.update(&id, patch)).await
}

#[tauri::command]
pub async fn instances_remove(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    let id = instance_id(&id)?;
    if state.launcher.active_instance().as_ref() == Some(&id) {
        return Err(IpcError::new(
            "instance_running",
            "Close Minecraft before deleting this instance.",
        ));
    }
    let settings = Arc::clone(&state.settings);
    if blocking(move || settings.get()).await?.active_instance_id == id.as_str() {
        return Err(IpcError::new(
            "instance_active",
            "Select another instance before deleting this one.",
        ));
    }
    let instances = Arc::clone(&state.instances);
    blocking(move || instances.remove(&id)).await
}

#[tauri::command]
pub async fn instances_get_active(state: State<'_, AppState>) -> IpcResult<Instance> {
    let (settings, instances) = (Arc::clone(&state.settings), Arc::clone(&state.instances));
    blocking(move || {
        let active = InstanceId::parse(&settings.get()?.active_instance_id)?;
        instances.get(&active)
    })
    .await
}

#[tauri::command]
pub async fn instances_set_active(state: State<'_, AppState>, id: String) -> IpcResult<Instance> {
    let (settings, instances, id) = (
        Arc::clone(&state.settings),
        Arc::clone(&state.instances),
        instance_id(&id)?,
    );
    blocking(move || {
        let instance = instances.get(&id)?;
        settings.modify(|current| {
            Ok(LauncherSettings {
                active_instance_id: id.to_string(),
                ..current.clone()
            })
        })?;
        Ok(instance)
    })
    .await
}

// ── launch ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn launch_get_state(state: State<'_, AppState>) -> LaunchStateEvent {
    state.launcher.state()
}

#[tauri::command]
pub async fn launch_start(state: State<'_, AppState>, instance_id: Option<String>) -> IpcResult<()> {
    let id = if let Some(raw) = instance_id {
        self::instance_id(&raw)?
    } else {
        let settings = Arc::clone(&state.settings);
        let active = blocking(move || settings.get()).await?.active_instance_id;
        InstanceId::parse(&active).map_err(|_| IpcError::new("missing_instance", "Select an instance first."))?
    };
    let credentials = state.accounts.credentials().await?;
    let auth = LaunchAuth {
        username: credentials.username,
        uuid: credentials.uuid,
        access_token: credentials.access_token,
        user_type: credentials.user_type.to_owned(),
        xuid: credentials.xuid,
        client_id: None,
    };
    state.launcher.launch(&id, auth, Arc::clone(&state.observer)).await?;
    Ok(())
}

#[tauri::command]
pub fn launch_stop(state: State<'_, AppState>) -> bool {
    state.launcher.stop(&state.observer)
}

// ── auth ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn auth_get_session(state: State<'_, AppState>) -> IpcResult<Option<Session>> {
    Ok(state.accounts.session().await)
}

#[tauri::command]
pub async fn auth_login_offline(state: State<'_, AppState>, username: String) -> IpcResult<Session> {
    let session = state.accounts.login_offline(&username).await?;
    let (settings, name) = (Arc::clone(&state.settings), session.username.clone());
    blocking(move || {
        settings.modify(|current| {
            let mut next = current.clone();
            next.user.offline_accounts.retain(|account| account != &name);
            next.user.offline_accounts.insert(0, name.clone());
            next.user.offline_accounts.truncate(MAX_OFFLINE_ACCOUNTS);
            next.user.last_offline_username = name;
            next.user.remember_offline_session = true;
            Ok(next)
        })
    })
    .await?;
    Ok(session)
}

#[tauri::command]
pub async fn auth_forget_offline(state: State<'_, AppState>, username: String) -> IpcResult<Option<Session>> {
    let settings = Arc::clone(&state.settings);
    let forgotten = username.clone();
    let next = blocking(move || {
        settings.modify(|current| {
            let mut next = current.clone();
            next.user.offline_accounts.retain(|account| account != &forgotten);
            if next.user.last_offline_username == forgotten {
                next.user.last_offline_username = next.user.offline_accounts.first().cloned().unwrap_or_default();
            }
            next.user.remember_offline_session = !next.user.last_offline_username.is_empty();
            Ok(next)
        })
    })
    .await?;

    let current = state.accounts.session().await;
    let removing_current = current
        .as_ref()
        .is_some_and(|session| session.mode == jifercraft_auth::AuthMode::Offline && session.username == username);
    if !removing_current {
        return Ok(current);
    }
    state.accounts.logout().await?;
    if next.user.last_offline_username.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            state.accounts.login_offline(&next.user.last_offline_username).await?,
        ))
    }
}

#[tauri::command]
pub async fn auth_start_microsoft(state: State<'_, AppState>) -> IpcResult<DeviceCode> {
    let code = state.accounts.start_microsoft().await?;
    let previous = state
        .pending_device_code
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .replace((code.clone(), CancellationToken::new()));
    if let Some((_, cancel)) = previous {
        cancel.cancel();
    }
    Ok(code)
}

#[tauri::command]
pub async fn auth_complete_microsoft(state: State<'_, AppState>, device_code: String) -> IpcResult<Session> {
    let pending = state
        .pending_device_code
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let Some((code, cancel)) = pending.filter(|(code, _)| code.device_code == device_code) else {
        return Err(IpcError::new(
            "microsoft_auth_expired",
            "Start the Microsoft sign-in again.",
        ));
    };
    let result = state.accounts.complete_microsoft(&code, &cancel).await;
    state
        .pending_device_code
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    Ok(result?)
}

#[tauri::command]
pub fn auth_cancel_microsoft(state: State<'_, AppState>) {
    if let Some((_, cancel)) = state
        .pending_device_code
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        cancel.cancel();
    }
}

#[tauri::command]
pub async fn auth_logout(state: State<'_, AppState>) -> IpcResult<()> {
    if state.launcher.state().state == LaunchState::Running {
        return Err(IpcError::new("already_running", "Close Minecraft before signing out."));
    }
    Ok(state.accounts.logout().await?)
}

// ── updates ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn updates_get_status(state: State<'_, AppState>) -> UpdateStatus {
    state.updates.status()
}

#[tauri::command]
pub async fn updates_check(state: State<'_, AppState>) -> IpcResult<()> {
    state.updates.check(CheckOrigin::Manual).await;
    Ok(())
}

#[tauri::command]
pub async fn updates_download(state: State<'_, AppState>) -> IpcResult<()> {
    state.updates.download().await;
    Ok(())
}

#[tauri::command]
pub fn updates_install(state: State<'_, AppState>) -> IpcResult<()> {
    Ok(state.updates.install()?)
}
