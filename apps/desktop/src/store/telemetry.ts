import type { DownloadTelemetry, LaunchPhase } from "../../shared/types/events";

/** Order in which the backend prepares a launch. */
export const launchPhases: readonly LaunchPhase[] = ["metadata", "files", "natives", "java", "starting"];

export type PhaseState = "done" | "active" | "pending";

export function phaseStates(current: LaunchPhase | null): Record<LaunchPhase, PhaseState> {
  const index = current === null ? -1 : launchPhases.indexOf(current);
  return Object.fromEntries(
    launchPhases.map((phase, position) => [phase, position < index ? "done" : position === index ? "active" : "pending"])
  ) as Record<LaunchPhase, PhaseState>;
}

/** 0-100 by bytes when sizes are known, by files otherwise; `null` when nothing is measurable yet. */
export function downloadPercent(download: DownloadTelemetry | undefined): number | null {
  if (!download) {
    return null;
  }

  if (download.bytesTotal > 0) {
    return clamp((download.bytesDone / download.bytesTotal) * 100);
  }

  return download.filesTotal > 0 ? clamp((download.filesDone / download.filesTotal) * 100) : null;
}

export function remainingSeconds(download: DownloadTelemetry | undefined): number | null {
  if (!download || download.bytesPerSecond <= 0 || download.bytesTotal <= 0) {
    return null;
  }

  const left = Math.max(0, download.bytesTotal - download.bytesDone);
  return Math.ceil(left / download.bytesPerSecond);
}

export function formatDuration(seconds: number, locale?: string): string {
  const safe = Math.max(0, Math.round(seconds));
  const unit = (value: number, name: "second" | "minute" | "hour") =>
    new Intl.NumberFormat(locale, { style: "unit", unit: name, unitDisplay: "narrow" }).format(value);

  if (safe < 60) {
    return unit(safe, "second");
  }

  if (safe < 3600) {
    const rest = safe % 60;
    return rest === 0 ? unit(Math.floor(safe / 60), "minute") : `${unit(Math.floor(safe / 60), "minute")} ${unit(rest, "second")}`;
  }

  return `${unit(Math.floor(safe / 3600), "hour")} ${unit(Math.floor((safe % 3600) / 60), "minute")}`;
}

function clamp(value: number): number {
  return Math.min(100, Math.max(0, value));
}
