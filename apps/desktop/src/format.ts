import type { LoaderType } from "../shared/types/instance";

const minute = 60_000;
const hour = 60 * minute;
const day = 24 * hour;
const relativeLimit = 30 * day;

const loaderLabels: Record<LoaderType, string> = {
  vanilla: "Vanilla",
  forge: "Forge",
  fabric: "Fabric",
  quilt: "Quilt",
  neoforge: "NeoForge"
};

export function loaderLabel(loaderType: LoaderType): string {
  return loaderLabels[loaderType];
}

export function formatRelativeTime(iso: string, now: number, locale: string): string {
  const time = Date.parse(iso);

  if (Number.isNaN(time)) {
    return "";
  }

  const elapsed = Math.max(now - time, 0);

  if (elapsed >= relativeLimit) {
    return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(time);
  }

  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });

  if (elapsed >= day) {
    return formatter.format(-Math.floor(elapsed / day), "day");
  }

  if (elapsed >= hour) {
    return formatter.format(-Math.floor(elapsed / hour), "hour");
  }

  if (elapsed >= minute) {
    return formatter.format(-Math.floor(elapsed / minute), "minute");
  }

  return formatter.format(0, "second");
}
