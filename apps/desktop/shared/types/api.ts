import type { LauncherLogEntry, LaunchProgress, LaunchStateEvent } from "./events";
import type { LauncherSession, MicrosoftDeviceCode } from "./auth";
import type { CloneInstanceInput, CreateInstanceInput, Instance, UpdateInstanceInput } from "./instance";
import type { LauncherSettings, LauncherSettingsUpdate } from "./settings";
import type { UpdateStatus } from "./updates";
import type { MinecraftVersion } from "./version";

export type Unsubscribe = () => void;

/** Error shape every backend command rejects with. */
export interface IpcError {
  code: string;
  message: string;
}

export type AccountNotice = "legacySessionDiscarded" | "microsoftExpired" | "microsoftUnavailable";

export interface AppBootstrap {
  version: string;
  microsoftAvailable: boolean;
  dataDir: string;
  notices: {
    account: AccountNotice | null;
    migratedDefaultInstance: boolean;
    onedriveDetected?: boolean;
  };
}

export interface LauncherApi {
  app: {
    bootstrap: () => Promise<AppBootstrap>;
    openExternal: (url: string) => Promise<void>;
    openDataFolder: () => Promise<void>;
    onSessionRestored: (handler: () => void) => Unsubscribe;
  };
  auth: {
    getSession: () => Promise<LauncherSession | null>;
    loginOffline: (username: string) => Promise<LauncherSession>;
    startMicrosoft: () => Promise<MicrosoftDeviceCode>;
    completeMicrosoft: (deviceCode: string) => Promise<LauncherSession>;
    cancelMicrosoft: () => Promise<void>;
    forgetOffline: (username: string) => Promise<LauncherSession | null>;
    logout: () => Promise<void>;
  };
  versions: {
    list: () => Promise<MinecraftVersion[]>;
  };
  instances: {
    list: () => Promise<Instance[]>;
    get: (id: string) => Promise<Instance>;
    create: (input: CreateInstanceInput) => Promise<Instance>;
    clone: (input: CloneInstanceInput) => Promise<Instance>;
    update: (id: string, patch: UpdateInstanceInput) => Promise<Instance>;
    remove: (id: string) => Promise<void>;
    getActive: () => Promise<Instance>;
    setActive: (id: string) => Promise<Instance>;
  };
  launch: {
    getState: () => Promise<LaunchStateEvent>;
    start: (instanceId?: string) => Promise<void>;
    stop: () => Promise<boolean>;
    onProgress: (handler: (event: LaunchProgress) => void) => Unsubscribe;
    onState: (handler: (event: LaunchStateEvent) => void) => Unsubscribe;
  };
  settings: {
    get: () => Promise<LauncherSettings>;
    update: (patch: LauncherSettingsUpdate) => Promise<LauncherSettings>;
  };
  updates: {
    getStatus: () => Promise<UpdateStatus>;
    check: () => Promise<void>;
    download: () => Promise<void>;
    install: () => Promise<void>;
    onStatus: (handler: (status: UpdateStatus) => void) => Unsubscribe;
  };
  logs: {
    getRecent: (limit?: number) => Promise<LauncherLogEntry[]>;
    /** Writes the text to a new file in the launcher's log folder and returns its path. */
    export: (contents: string) => Promise<string>;
    onBatch: (handler: (entries: LauncherLogEntry[]) => void) => Unsubscribe;
  };
}
