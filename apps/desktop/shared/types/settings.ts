export interface LauncherSettings {
  activeInstanceId: string;
  /** @deprecated Lives in `Instance.versionId`. Kept only to migrate existing settings. */
  selectedVersion: string;
  minecraftRoot: string;
  /** @deprecated Lives in `Instance.javaPath`. Kept only to migrate existing settings. */
  javaPath: string;
  /** @deprecated Lives in `Instance.memoryConfig`. Kept only to migrate existing settings. */
  memory: {
    min: string;
    max: string;
  };
  window: {
    width: number;
    height: number;
    fullscreen: boolean;
  };
  user: {
    lastOfflineUsername: string;
    rememberOfflineSession: boolean;
    offlineAccounts: string[];
  };
  ui: {
    language: "es" | "en";
    versionFilter: "release" | "snapshot" | "all";
    versionSearch: string;
  };
  history: {
    /** @deprecated Derived from `Instance.lastPlayedAt`. Kept only to migrate existing settings. */
    playedVersions: string[];
  };
}

/** Mirrors the backend `SettingsPatch`: every field is validated server-side; legacy fields are read-only. */
export interface LauncherSettingsUpdate {
  activeInstanceId?: string;
  minecraftRoot?: string;
  window?: LauncherSettings["window"];
  user?: Partial<LauncherSettings["user"]>;
  ui?: Partial<LauncherSettings["ui"]>;
}
