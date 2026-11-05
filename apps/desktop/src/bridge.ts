import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import type { LauncherApi, Unsubscribe } from "../shared/types/api";

// Tauri's listen() is async, but React hooks need synchronous unsubs.
// If the component unmounts before the listener finishes wiring up, queue the cleanup.
function subscribe<T>(event: string, handler: (payload: T) => void): Unsubscribe {
  let disposed = false;
  let unlisten: (() => void) | null = null;
  const callback: EventCallback<T> = (message) => handler(message.payload);

  void listen<T>(event, callback).then((stop) => {
    if (disposed) {
      stop();
    } else {
      unlisten = stop;
    }
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}

export const launcher: LauncherApi = {
  app: {
    bootstrap: () => invoke("app_bootstrap"),
    openExternal: (url) => invoke("app_open_external", { url }),
    openDataFolder: () => invoke("app_open_data_folder"),
    onSessionRestored: (handler) => subscribe<null>("auth:restored", () => handler())
  },
  auth: {
    getSession: () => invoke("auth_get_session"),
    loginOffline: (username) => invoke("auth_login_offline", { username }),
    startMicrosoft: () => invoke("auth_start_microsoft"),
    completeMicrosoft: (deviceCode) => invoke("auth_complete_microsoft", { deviceCode }),
    cancelMicrosoft: () => invoke("auth_cancel_microsoft"),
    forgetOffline: (username) => invoke("auth_forget_offline", { username }),
    logout: () => invoke("auth_logout")
  },
  versions: {
    list: () => invoke("versions_list")
  },
  instances: {
    list: () => invoke("instances_list"),
    get: (id) => invoke("instances_get", { id }),
    create: (input) => invoke("instances_create", { input }),
    clone: (input) => invoke("instances_clone", { input }),
    update: (id, patch) => invoke("instances_update", { id, patch }),
    remove: (id) => invoke("instances_remove", { id }),
    getActive: () => invoke("instances_get_active"),
    setActive: (id) => invoke("instances_set_active", { id })
  },
  launch: {
    getState: () => invoke("launch_get_state"),
    start: (instanceId) => invoke("launch_start", { instanceId: instanceId ?? null }),
    stop: () => invoke("launch_stop"),
    onProgress: (handler) => subscribe("launch:progress", handler),
    onState: (handler) => subscribe("launch:state", handler)
  },
  settings: {
    get: () => invoke("settings_get"),
    update: (patch) => invoke("settings_update", { patch })
  },
  updates: {
    getStatus: () => invoke("updates_get_status"),
    check: () => invoke("updates_check"),
    download: () => invoke("updates_download"),
    install: () => invoke("updates_install"),
    onStatus: (handler) => subscribe("updates:status", handler)
  },
  logs: {
    getRecent: (limit) => invoke("logs_get_recent", { limit: limit ?? null }),
    export: (contents) => invoke("logs_export", { contents }),
    onBatch: (handler) => subscribe("logs:batch", handler)
  }
};
