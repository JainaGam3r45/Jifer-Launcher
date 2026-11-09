import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppBootstrap } from "../../shared/types/api";
import type { LaunchProgress, LaunchStateEvent } from "../../shared/types/events";
import type { LauncherSession, MicrosoftDeviceCode } from "../../shared/types/auth";
import type { LauncherSettings, LauncherSettingsUpdate } from "../../shared/types/settings";
import type { CloneInstanceInput, CreateInstanceInput, Instance, UpdateInstanceInput } from "../../shared/types/instance";
import type { UpdateStatus } from "../../shared/types/updates";
import type { MinecraftVersion } from "../../shared/types/version";
import { appInfo } from "../../shared/constants/app";
import { launcher } from "../bridge";
import { launcherCopy } from "../i18n";
import { errorCode, formatError } from "./errors";
import { logStore } from "./logstore";
import { useInstances } from "./useinstances";

const idleState: LaunchStateEvent = { state: "idle", message: "Ready." };

export function useAppState() {
  const [bootstrap, setBootstrap] = useState<AppBootstrap | null>(null);
  const [session, setSession] = useState<LauncherSession | null>(null);
  const [settings, setSettings] = useState<LauncherSettings | null>(null);
  const [versions, setVersions] = useState<MinecraftVersion[]>([]);
  const [status, setStatus] = useState<LaunchStateEvent>(idleState);
  const [progress, setProgress] = useState<LaunchProgress | null>(null);
  const [rawError, setRawError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>(initialUpdateStatus);
  const [microsoftCode, setMicrosoftCode] = useState<MicrosoftDeviceCode | null>(null);
  const [authLoading, setAuthLoading] = useState(false);

  const handleActiveChange = useCallback((instance: Instance) => {
    setSettings((current) => (current ? { ...current, activeInstanceId: instance.id } : current));
  }, []);
  const instanceState = useInstances({ onActiveChange: handleActiveChange });
  const { activeInstance, loadInstances, refreshInstance } = instanceState;

  const language = settings?.ui.language ?? "es";
  const error = useMemo(() => (rawError === null ? null : formatError(rawError, launcherCopy[language].errors)), [language, rawError]);

  const selectedVersion = useMemo(() => {
    return versions.find((version) => version.id === activeInstance?.versionId) ?? null;
  }, [activeInstance?.versionId, versions]);

  const refresh = useCallback(async () => {
    setLoading(true);
    setRawError(null);

    try {
      const [nextBootstrap, nextSession, nextSettings, nextState, recentLogs, nextUpdateStatus] = await Promise.all([
        launcher.app.bootstrap(),
        launcher.auth.getSession(),
        launcher.settings.get(),
        launcher.launch.getState(),
        launcher.logs.getRecent(),
        launcher.updates.getStatus()
      ]);
      await loadInstances();
      setBootstrap(nextBootstrap);
      setSession(nextSession);
      setSettings(nextSettings);
      setStatus(nextState);
      setUpdateStatus(nextUpdateStatus);
      logStore.replace(recentLogs);
    } catch (nextError) {
      setRawError(nextError);
    } finally {
      setLoading(false);
    }

    // The version list needs the network; the launcher stays usable without it.
    try {
      setVersions(await launcher.versions.list());
    } catch (nextError) {
      setRawError(nextError);
    }
  }, [loadInstances]);

  useEffect(() => {
    void refresh();

    const unsubscribers = [
      launcher.launch.onProgress(setProgress),
      launcher.launch.onState((event) => {
        setStatus(event);

        if (event.state !== "installing") {
          setProgress(null);
        }

        // Running records the last-played time; idle and error may follow a deletion or a failed launch.
        if (event.state === "running" || event.state === "error" || event.state === "idle") {
          void loadInstances();
        }
      }),
      launcher.logs.onBatch((entries) => logStore.append(entries)),
      launcher.updates.onStatus(setUpdateStatus),
      launcher.app.onSessionRestored(() => {
        void Promise.all([launcher.auth.getSession(), launcher.app.bootstrap()]).then(([nextSession, nextBootstrap]) => {
          setSession(nextSession);
          setBootstrap(nextBootstrap);
        });
      })
    ];

    return () => unsubscribers.forEach((unsubscribe) => unsubscribe());
  }, [loadInstances, refresh]);

  // A fresh instance has no version yet; give it the newest release once the list is known.
  useEffect(() => {
    if (!activeInstance || activeInstance.versionId || versions.length === 0) {
      return;
    }

    const latestRelease = versions.find((version) => version.type === "release");

    if (latestRelease) {
      void instanceState.updateInstance(activeInstance.id, { versionId: latestRelease.id }).catch(() => undefined);
    }
  }, [activeInstance, instanceState.updateInstance, versions]);

  const guard = useCallback(
    async <T>(action: () => Promise<T>): Promise<T | null> => {
      setRawError(null);

      try {
        return await action();
      } catch (nextError) {
        setRawError(nextError);
        return null;
      }
    },
    []
  );

  const withAuthLoading = useCallback(
    async (action: () => Promise<void>) => {
      setAuthLoading(true);

      try {
        await guard(action);
      } finally {
        setAuthLoading(false);
      }
    },
    [guard]
  );

  const syncSettings = useCallback(async () => setSettings(await launcher.settings.get()), []);

  const login = useCallback(
    (username: string) =>
      withAuthLoading(async () => {
        setSession(await launcher.auth.loginOffline(username));
        await syncSettings();
      }),
    [syncSettings, withAuthLoading]
  );

  const startMicrosoftLogin = useCallback(
    () =>
      withAuthLoading(async () => {
        const code = await launcher.auth.startMicrosoft();
        setMicrosoftCode(code);
        await launcher.app.openExternal(code.verificationUri);
      }),
    [withAuthLoading]
  );

  const completeMicrosoftLogin = useCallback(() => {
    if (!microsoftCode) {
      return Promise.resolve();
    }

    return withAuthLoading(async () => {
      const nextSession = await launcher.auth.completeMicrosoft(microsoftCode.deviceCode);
      setMicrosoftCode(null);
      setSession(nextSession);
    });
  }, [microsoftCode, withAuthLoading]);

  const cancelMicrosoftLogin = useCallback(async () => {
    setMicrosoftCode(null);
    await launcher.auth.cancelMicrosoft();
  }, []);

  const logout = useCallback(async () => {
    await guard(async () => {
      await launcher.auth.logout();
      setSession(null);
    });
  }, [guard]);

  const switchOfflineAccount = useCallback(
    async (username: string) => {
      await guard(async () => {
        setSession(await launcher.auth.loginOffline(username));
        await syncSettings();
      });
    },
    [guard, syncSettings]
  );

  const forgetOfflineAccount = useCallback(
    async (username: string) => {
      await guard(async () => {
        setSession(await launcher.auth.forgetOffline(username));
        await syncSettings();
      });
    },
    [guard, syncSettings]
  );

  const updateSettings = useCallback(
    async (update: LauncherSettingsUpdate): Promise<boolean> => {
      const next = await guard(() => launcher.settings.update(update));

      if (next) {
        setSettings(next);
      }

      return next !== null;
    },
    [guard]
  );

  const selectVersion = useCallback(
    async (versionId: string) => {
      if (activeInstance) {
        await guard(() => instanceState.updateInstance(activeInstance.id, { versionId }));
        await refreshInstance(activeInstance.id);
      }
    },
    [activeInstance, guard, instanceState.updateInstance, refreshInstance]
  );

  const launch = useCallback(async () => {
    setRawError(null);

    try {
      await launcher.launch.start(activeInstance?.id);
    } catch (nextError) {
      // Pressing Stop while files download rejects the pending launch; that is not an error to the user.
      if (errorCode(nextError) !== "cancelled") {
        setRawError(nextError);
      }
    }
  }, [activeInstance?.id]);

  const stop = useCallback(async () => {
    await guard(() => launcher.launch.stop());
  }, [guard]);

  const setActiveInstance = useCallback((id: string) => guard(() => instanceState.setActiveInstance(id)), [guard, instanceState.setActiveInstance]);
  const createInstance = useCallback((input: CreateInstanceInput) => guard(() => instanceState.createInstance(input)), [guard, instanceState.createInstance]);
  const updateInstance = useCallback(
    (id: string, patch: UpdateInstanceInput) => guard(() => instanceState.updateInstance(id, patch)),
    [guard, instanceState.updateInstance]
  );
  const cloneInstance = useCallback((input: CloneInstanceInput) => guard(() => instanceState.cloneInstance(input)), [guard, instanceState.cloneInstance]);
  const removeInstance = useCallback(
    (id: string) =>
      guard(async () => {
        await instanceState.removeInstance(id);
        return true;
      }),
    [guard, instanceState.removeInstance]
  );
  const dismissError = useCallback(() => setRawError(null), []);
  const checkForUpdates = useCallback(async () => {
    await guard(() => launcher.updates.check());
  }, [guard]);
  const downloadUpdate = useCallback(async () => {
    await guard(() => launcher.updates.download());
  }, [guard]);
  const installUpdate = useCallback(async () => {
    await guard(() => launcher.updates.install());
  }, [guard]);
  const openExternal = useCallback(async (url: string) => {
    await guard(() => launcher.app.openExternal(url));
  }, [guard]);
  const openDataFolder = useCallback(async () => {
    await guard(() => launcher.app.openDataFolder());
  }, [guard]);

  return {
    bootstrap,
    session,
    settings,
    versions,
    status,
    progress,
    error,
    loading,
    microsoftCode,
    authLoading,
    selectedVersion,
    instances: instanceState.instances,
    activeInstance,
    instancesError: instanceState.instancesError,
    setActiveInstance,
    createInstance,
    updateInstance,
    cloneInstance,
    removeInstance,
    dismissError,
    updateStatus,
    checkForUpdates,
    downloadUpdate,
    installUpdate,
    login,
    startMicrosoftLogin,
    completeMicrosoftLogin,
    cancelMicrosoftLogin,
    logout,
    switchOfflineAccount,
    forgetOfflineAccount,
    updateSettings,
    selectVersion,
    launch,
    stop,
    openExternal,
    openDataFolder,
    refresh
  };
}

const initialUpdateStatus: UpdateStatus = {
  state: "idle",
  supported: false,
  currentVersion: appInfo.version,
  info: null,
  progress: null,
  error: null,
  lastCheckedAt: null
};
