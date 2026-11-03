export type LauncherState = "idle" | "installing" | "launching" | "running" | "stopping" | "error";

export interface LaunchStateEvent {
  state: LauncherState;
  message: string;
  instanceId?: string;
  /** Stable error code when `state` is `error`. */
  code?: string;
}

export type LaunchPhase = "metadata" | "java" | "files" | "natives" | "starting";

/** Live counters from the verified downloader. Bytes are totals of files with a known size. */
export interface DownloadTelemetry {
  filesTotal: number;
  filesDone: number;
  filesReused: number;
  filesActive: number;
  filesVerifying: number;
  filesVerified: number;
  retries: number;
  bytesTotal: number;
  bytesDone: number;
  bytesPerSecond: number;
}

export interface LaunchProgress {
  phase: LaunchPhase;
  message: string;
  download?: DownloadTelemetry;
}

export type LogLevel = "debug" | "info" | "warn" | "error";
export type LogSource = "launcher" | "game";

export interface LauncherLogEntry {
  id: number;
  timestamp: string;
  level: LogLevel;
  source: LogSource;
  message: string;
}
