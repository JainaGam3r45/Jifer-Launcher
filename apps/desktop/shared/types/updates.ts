export type UpdateState = "idle" | "checking" | "available" | "downloading" | "downloaded" | "error";

export interface UpdateInfo {
  version: string;
  releaseDate?: string;
  notes: string | null;
}

export interface UpdateProgress {
  percent: number;
  transferred: number;
  total: number;
  bytesPerSecond: number;
}

export interface UpdateStatus {
  state: UpdateState;
  supported: boolean;
  currentVersion: string;
  info: UpdateInfo | null;
  progress: UpdateProgress | null;
  error: string | null;
  lastCheckedAt: string | null;
}
