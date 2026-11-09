import type { UpdateProgress, UpdateState, UpdateStatus } from "../../shared/types/updates";

const visibleStates: readonly UpdateState[] = ["available", "downloading", "downloaded", "error"];
const byteUnits = ["byte", "kilobyte", "megabyte", "gigabyte"] as const;
const unitSize = 1024;

export function shouldShowUpdateIndicator(status: UpdateStatus): boolean {
  return status.supported && visibleStates.includes(status.state);
}

export function formatBytes(bytes: number, locale?: string): string {
  const safeBytes = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  let index = 0;

  while (index < byteUnits.length - 1 && safeBytes >= unitSize ** (index + 1)) {
    index += 1;
  }

  return new Intl.NumberFormat(locale, {
    style: "unit",
    unit: byteUnits[index],
    unitDisplay: "short",
    maximumFractionDigits: index === 0 ? 0 : 1
  }).format(safeBytes / unitSize ** index);
}

export function formatSpeed(bytesPerSecond: number, locale?: string): string {
  return `${formatBytes(bytesPerSecond, locale)}/s`;
}

export function resolvePercent(progress: UpdateProgress | null): number {
  if (!progress || !Number.isFinite(progress.percent)) {
    return 0;
  }

  return Math.min(100, Math.max(0, Math.round(progress.percent)));
}

export function resolveProgressSummary(progress: UpdateProgress | null, locale?: string): string {
  if (!progress) {
    return "";
  }

  const parts = [new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 0 }).format(resolvePercent(progress) / 100)];

  if (progress.total > 0) {
    parts.push(`${formatBytes(progress.transferred, locale)} / ${formatBytes(progress.total, locale)}`);
  }

  if (progress.bytesPerSecond > 0) {
    parts.push(formatSpeed(progress.bytesPerSecond, locale));
  }

  return parts.join(" · ");
}

export function formatReleaseDate(iso: string | undefined, locale?: string): string {
  const time = iso ? Date.parse(iso) : Number.NaN;
  return Number.isNaN(time) ? "" : new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(time);
}

export interface UpdateStateCopy {
  unchecked: string;
  upToDate: string;
  checking: string;
  available: (version: string) => string;
  downloading: (percent: number) => string;
  downloaded: (version: string) => string;
  error: string;
}

export function describeUpdateState(status: UpdateStatus, copy: UpdateStateCopy): string {
  const version = status.info?.version ?? "";

  switch (status.state) {
    case "checking":
      return copy.checking;
    case "available":
      return copy.available(version);
    case "downloading":
      return copy.downloading(resolvePercent(status.progress));
    case "downloaded":
      return copy.downloaded(version);
    case "error":
      return copy.error;
    default:
      return status.lastCheckedAt ? copy.upToDate : copy.unchecked;
  }
}
