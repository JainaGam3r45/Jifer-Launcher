import type { LauncherLogEntry, LogLevel, LogSource } from "../../shared/types/events";

export type LevelFilter = LogLevel | "all";
export type SourceFilter = LogSource | "all";

export interface ConsoleFilter {
  level: LevelFilter;
  source: SourceFilter;
  query: string;
}

const severity: Record<LogLevel, number> = { debug: 0, info: 1, warn: 2, error: 3 };

/** A level filter means "this level and anything more severe". The query is case-insensitive. */
export function filterEntries(entries: readonly LauncherLogEntry[], filter: ConsoleFilter): readonly LauncherLogEntry[] {
  const query = filter.query.trim().toLowerCase();
  const minimum = filter.level === "all" ? -1 : severity[filter.level];

  if (minimum < 0 && filter.source === "all" && !query) {
    return entries;
  }

  return entries.filter(
    (entry) =>
      severity[entry.level] >= minimum &&
      (filter.source === "all" || entry.source === filter.source) &&
      (!query || entry.message.toLowerCase().includes(query))
  );
}

export function countByLevel(entries: readonly LauncherLogEntry[]): Record<LogLevel, number> {
  const counts: Record<LogLevel, number> = { debug: 0, info: 0, warn: 0, error: 0 };
  for (const entry of entries) {
    counts[entry.level] += 1;
  }
  return counts;
}

export function formatEntry(entry: LauncherLogEntry): string {
  return `${entry.timestamp} ${entry.level.toUpperCase().padEnd(5)} [${entry.source}] ${entry.message}`;
}

export function formatForExport(entries: readonly LauncherLogEntry[]): string {
  return entries.map(formatEntry).join("\n") + (entries.length > 0 ? "\n" : "");
}

/** Rows to render for a fixed-height virtual list, with a small overscan on both sides. */
export function visibleRange(scrollTop: number, viewportHeight: number, rowHeight: number, total: number, overscan = 8) {
  const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const last = Math.min(total, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan);
  return { first, last: Math.max(first, last) };
}
