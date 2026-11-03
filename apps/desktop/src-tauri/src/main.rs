// No console window next to the launcher in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod error;
mod logs;
mod secrets;
mod state;
mod updater;

use std::sync::Arc;

use jifercraft_core::LauncherPaths;
use jifercraft_core::settings::LauncherSettings;
use tauri::{Emitter, Manager};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::logs::{HubLayer, LogHub};
use crate::state::{AppState, UPDATE_EVENT};

const DATA_DIR_OVERRIDE: &str = "JIFERCRAFT_DATA_DIR";

fn main() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch focuses the existing window instead of racing it.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let paths = match std::env::var_os(DATA_DIR_OVERRIDE) {
                // Portable installs and isolated test runs.
                Some(dir) if !dir.is_empty() => LauncherPaths::at(dir),
                // `config_dir` is what Electron called `appData` on every platform (%APPDATA%, ~/.config,
                // ~/Library/Application Support), so existing installs keep their `.jifercloud` folder.
                _ => LauncherPaths::from_app_data(app.path().config_dir()?),
            };
            let logs = Arc::new(LogHub::default());
            let guard = init_tracing(&paths, &logs);
            app.manage(guard);

            let state = AppState::build(&handle, paths, Arc::clone(&logs))?;
            startup(&state);
            logs.start_flushing(handle.clone());
            state.updates.subscribe({
                let handle = handle.clone();
                move |status| {
                    let _ = handle.emit(UPDATE_EVENT, status);
                }
            });
            state.updates.schedule_checks();
            app.manage(state);
            restore_session(&handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_bootstrap,
            commands::app_open_external,
            commands::app_open_data_folder,
            commands::logs_get_recent,
            commands::logs_export,
            commands::settings_get,
            commands::settings_update,
            commands::versions_list,
            commands::instances_list,
            commands::instances_get,
            commands::instances_create,
            commands::instances_clone,
            commands::instances_update,
            commands::instances_remove,
            commands::instances_get_active,
            commands::instances_set_active,
            commands::launch_get_state,
            commands::launch_start,
            commands::launch_stop,
            commands::auth_get_session,
            commands::auth_login_offline,
            commands::auth_forget_offline,
            commands::auth_start_microsoft,
            commands::auth_complete_microsoft,
            commands::auth_cancel_microsoft,
            commands::auth_logout,
            commands::updates_get_status,
            commands::updates_check,
            commands::updates_download,
            commands::updates_install,
        ])
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(error) => {
            eprintln!("JiferCraft Launcher could not start: {error}");
            #[cfg(windows)]
            show_fatal_dialog(&format!("JiferCraft Launcher no pudo arrancar:\n\n{error}"));
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn show_fatal_dialog(message: &str) {
    // Si la app muere antes de crear la ventana gráfica, evitamos la típica muerte silenciosa
    // de Windows donde el usuario hace doble clic y parece que el ejecutable pasa olímpicamente de él.
    // Ojo: en Windows las rutas llevan barras invertidas (ej. C:\Users) que en JS detonan como
    // secuencias Unicode inválidas (\U) en Trident/MSHTML. Escapamos las contrabarras primero.
    let sanitized_js = message
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('\r', "")
        .replace('\n', "\\n");
    let script = format!("javascript:alert('{sanitized_js}');close();");
    let mshta_ok = std::process::Command::new("mshta")
        .arg(script)
        .status()
        .is_ok_and(|s| s.success());

    // Si mshta está capado por directivas de grupo, AppLocker o SmartApp Control,
    // recurrimos a PowerShell y System.Windows.Forms antes de tirar la toalla.
    if !mshta_ok {
        let ps_msg = message.replace('\'', "''");
        let _ = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('{ps_msg}', 'JiferCraft Launcher', 0, 16)"
                ),
            ])
            .status();
    }
}

/// File log (daily, 7 kept) plus the in-app diagnostics console. The guard flushes the file on exit.
fn init_tracing(paths: &LauncherPaths, logs: &Arc<LogHub>) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = || EnvFilter::try_from_env("JIFERCRAFT_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("launcher")
        .filename_suffix("log")
        .max_log_files(7)
        .build(paths.logs_dir());
    let (file_layer, guard) = match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
                .with_filter(filter());
            (Some(layer), Some(guard))
        }
        Err(error) => {
            eprintln!("launcher log file unavailable: {error}");
            (None, None)
        }
    };
    let console = cfg!(debug_assertions).then(|| tracing_subscriber::fmt::layer().with_filter(filter()));
    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(console)
        .with(HubLayer(Arc::clone(logs)).with_filter(filter()))
        .try_init();
    guard
}

/// Synchronous startup: legacy Electron leftovers and the active instance.
fn startup(state: &AppState) {
    let legacy = jifercraft_auth::discard_legacy_session(&state.paths.legacy_session_file());
    let is_onedrive = jifercraft_core::paths::is_onedrive_path(state.paths.root());
    let settings = match state.settings.get() {
        Ok(settings) => settings,
        Err(error) => {
            tracing::error!(%error, "settings could not be loaded");
            let mut notices = state.notices.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            notices.onedrive_detected = is_onedrive;
            return;
        }
    };
    match state.instances.ensure_default(&settings) {
        Ok(result) => {
            if settings.active_instance_id != result.instance.id.as_str()
                && let Err(error) = state.settings.modify(|current| {
                    Ok(LauncherSettings {
                        active_instance_id: result.instance.id.to_string(),
                        ..current.clone()
                    })
                })
            {
                tracing::warn!(%error, "could not store the active instance");
            }
            let mut notices = state.notices.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            notices.migrated_default_instance = result.moved_entries > 0;
            notices.onedrive_detected = is_onedrive;
            if legacy {
                notices.account = Some(state::AccountNotice::LegacySessionDiscarded);
            }
        }
        Err(error) => {
            tracing::error!(%error, "could not prepare the default instance");
            let mut notices = state.notices.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            notices.onedrive_detected = is_onedrive;
        }
    }
}

/// Restores the account in the background so the window is not held up by Microsoft's servers.
fn restore_session(handle: &tauri::AppHandle) {
    let state = handle.state::<AppState>();
    let accounts = Arc::clone(&state.accounts);
    let remembered = state.settings.get().ok().and_then(|settings| {
        let user = settings.user;
        let name = if user.last_offline_username.is_empty() {
            user.offline_accounts.first().cloned().unwrap_or_default()
        } else {
            user.last_offline_username
        };
        (user.remember_offline_session && !name.is_empty()).then_some(name)
    });
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = accounts.restore(remembered.as_deref()).await;
        if let Some(state) = handle.try_state::<AppState>() {
            state.record_restore(outcome);
        }
        let _ = handle.emit("auth:restored", ());
    });
}
