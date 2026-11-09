import { describe, expect, test } from "bun:test";
import type { LauncherLogEntry, LogLevel, LogSource } from "../shared/types/events";
import { countByLevel, filterEntries, formatForExport, visibleRange } from "../src/store/consolefilter";
import { LogStore } from "../src/store/logstore";

let nextId = 0;

function entry(level: LogLevel, source: LogSource, message: string): LauncherLogEntry {
  nextId += 1;
  return { id: nextId, timestamp: "2026-11-10T10:00:00.000Z", level, source, message };
}

const entries = [
  entry("debug", "game", "Loading chunk"),
  entry("info", "launcher", "Starting Minecraft 1.20.1"),
  entry("warn", "game", "Missing texture"),
  entry("error", "game", "java.lang.OutOfMemoryError: Java heap space")
];

describe("filterEntries", () => {
  test("returns the same array when nothing is filtered", () => {
    expect(filterEntries(entries, { level: "all", source: "all", query: "" })).toBe(entries);
  });

  test("treats the level as a minimum severity", () => {
    expect(filterEntries(entries, { level: "warn", source: "all", query: "" }).map((e) => e.level)).toEqual(["warn", "error"]);
  });

  test("combines source and case-insensitive text", () => {
    expect(filterEntries(entries, { level: "all", source: "game", query: "  HEAP " }).map((e) => e.message)).toEqual([
      "java.lang.OutOfMemoryError: Java heap space"
    ]);
    expect(filterEntries(entries, { level: "all", source: "launcher", query: "texture" })).toEqual([]);
  });
});

describe("export and counts", () => {
  test("formats one aligned line per entry", () => {
    expect(formatForExport(entries.slice(1, 3))).toBe(
      "2026-11-10T10:00:00.000Z INFO  [launcher] Starting Minecraft 1.20.1\n2026-11-10T10:00:00.000Z WARN  [game] Missing texture\n"
    );
    expect(formatForExport([])).toBe("");
  });

  test("counts by level", () => {
    expect(countByLevel(entries)).toEqual({ debug: 1, info: 1, warn: 1, error: 1 });
  });
});

describe("visibleRange", () => {
  test("windows the list around the viewport with overscan", () => {
    expect(visibleRange(0, 220, 22, 10_000, 5)).toEqual({ first: 0, last: 15 });
    expect(visibleRange(22_000, 220, 22, 10_000, 5)).toEqual({ first: 995, last: 1015 });
    expect(visibleRange(219_780, 220, 22, 10_000, 5)).toEqual({ first: 9985, last: 10_000 });
    expect(visibleRange(0, 220, 22, 0, 5)).toEqual({ first: 0, last: 0 });
  });
});

describe("LogStore", () => {
  test("keeps the newest entries and batches notifications per frame", () => {
    const frames: (() => void)[] = [];
    const store = new LogStore(3, (callback) => frames.push(callback));
    let notified = 0;
    store.subscribe(() => {
      notified += 1;
    });

    store.append(entries.slice(0, 2));
    store.append(entries.slice(2));
    expect(store.snapshot().map((e) => e.message)).toEqual(["Starting Minecraft 1.20.1", "Missing texture", "java.lang.OutOfMemoryError: Java heap space"]);
    expect(frames).toHaveLength(1);

    frames.shift()?.();
    expect(notified).toBe(1);

    store.clear();
    frames.shift()?.();
    expect(store.snapshot()).toEqual([]);
    expect(notified).toBe(2);
  });
});
