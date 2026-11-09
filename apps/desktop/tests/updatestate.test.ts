import { describe, expect, test } from "bun:test";
import {
  describeUpdateState,
  formatBytes,
  formatReleaseDate,
  formatSpeed,
  resolvePercent,
  resolveProgressSummary,
  shouldShowUpdateIndicator,
  type UpdateStateCopy
} from "../src/store/updatestate";
import type { UpdateProgress, UpdateState, UpdateStatus } from "../shared/types/updates";

function makeStatus(state: UpdateState, overrides: Partial<UpdateStatus> = {}): UpdateStatus {
  return { state, supported: true, currentVersion: "0.1.0", info: null, progress: null, error: null, lastCheckedAt: null, ...overrides };
}

function makeProgress(overrides: Partial<UpdateProgress> = {}): UpdateProgress {
  return { percent: 0, transferred: 0, total: 0, bytesPerSecond: 0, ...overrides };
}

const normalize = (value: string): string => value.replace(/[  ]/g, " ");

describe("shouldShowUpdateIndicator", () => {
  test.each([
    ["available", true],
    ["downloading", true],
    ["downloaded", true],
    ["error", true],
    ["idle", false],
    ["checking", false]
  ] as const)("is %s -> %p", (state, expected) => {
    expect(shouldShowUpdateIndicator(makeStatus(state))).toBe(expected);
  });

  test("stays hidden when updates are not supported", () => {
    expect(shouldShowUpdateIndicator(makeStatus("available", { supported: false }))).toBe(false);
  });
});

describe("formatBytes", () => {
  test("picks the unit and rounds to one decimal", () => {
    expect(formatBytes(0, "en")).toBe("0 byte");
    expect(formatBytes(512, "en")).toBe("512 byte");
    expect(formatBytes(1536, "en")).toBe("1.5 kB");
    expect(formatBytes(12.3 * 1024 * 1024, "en")).toBe("12.3 MB");
    expect(formatBytes(3 * 1024 ** 3, "en")).toBe("3 GB");
  });

  test("switches unit exactly at the boundary", () => {
    expect(formatBytes(1023, "en")).toBe("1,023 byte");
    expect(formatBytes(1024, "en")).toBe("1 kB");
    expect(formatBytes(1024 ** 2, "en")).toBe("1 MB");
  });

  test("uses the locale decimal separator", () => {
    expect(normalize(formatBytes(12.3 * 1024 * 1024, "es"))).toBe("12,3 MB");
  });

  test("treats negative and non-finite values as zero", () => {
    expect(formatBytes(-5, "en")).toBe("0 byte");
    expect(formatBytes(Number.NaN, "en")).toBe("0 byte");
    expect(formatBytes(Number.POSITIVE_INFINITY, "en")).toBe("0 byte");
  });
});

describe("formatSpeed", () => {
  test("appends the per-second suffix", () => {
    expect(formatSpeed(2.1 * 1024 * 1024, "en")).toBe("2.1 MB/s");
    expect(formatSpeed(0, "en")).toBe("0 byte/s");
  });
});

describe("resolvePercent", () => {
  test("rounds and clamps", () => {
    expect(resolvePercent(makeProgress({ percent: 44.6 }))).toBe(45);
    expect(resolvePercent(makeProgress({ percent: 140 }))).toBe(100);
    expect(resolvePercent(makeProgress({ percent: -3 }))).toBe(0);
    expect(resolvePercent(makeProgress({ percent: Number.NaN }))).toBe(0);
    expect(resolvePercent(null)).toBe(0);
  });
});

describe("resolveProgressSummary", () => {
  test("combines percentage, transferred size and speed", () => {
    const summary = resolveProgressSummary(
      makeProgress({ percent: 45.4, transferred: 12.3 * 1024 * 1024, total: 27 * 1024 * 1024, bytesPerSecond: 2.1 * 1024 * 1024 }),
      "en"
    );

    expect(summary).toBe("45% · 12.3 MB / 27 MB · 2.1 MB/s");
  });

  test("is empty without progress", () => {
    expect(resolveProgressSummary(null, "en")).toBe("");
  });

  test("shows only the percentage when nothing has been measured yet", () => {
    expect(resolveProgressSummary(makeProgress(), "en")).toBe("0%");
  });

  test("omits the speed when it is zero and the size when the total is unknown", () => {
    expect(resolveProgressSummary(makeProgress({ percent: 50, transferred: 1024, total: 2048 }), "en")).toBe("50% · 1 kB / 2 kB");
    expect(resolveProgressSummary(makeProgress({ percent: 50, bytesPerSecond: 1024 }), "en")).toBe("50% · 1 kB/s");
  });

  test("formats the percentage for the locale", () => {
    expect(normalize(resolveProgressSummary(makeProgress({ percent: 45 }), "es"))).toBe("45 %");
  });
});

describe("formatReleaseDate", () => {
  test("formats a valid date and ignores missing or invalid ones", () => {
    expect(formatReleaseDate("2026-11-01T12:00:00.000Z", "en")).toBe("Nov 1, 2026");
    expect(formatReleaseDate(undefined, "en")).toBe("");
    expect(formatReleaseDate("not a date", "en")).toBe("");
  });
});

describe("describeUpdateState", () => {
  const copy: UpdateStateCopy = {
    unchecked: "unchecked",
    upToDate: "up to date",
    checking: "checking",
    available: (version) => `available ${version}`,
    downloading: (percent) => `downloading ${percent}`,
    downloaded: (version) => `downloaded ${version}`,
    error: "error"
  };
  const info = { version: "0.2.0", notes: null };

  test("idle distinguishes a first run from a completed check", () => {
    expect(describeUpdateState(makeStatus("idle"), copy)).toBe("unchecked");
    expect(describeUpdateState(makeStatus("idle", { lastCheckedAt: "2026-11-10T10:00:00.000Z" }), copy)).toBe("up to date");
  });

  test("names the pending version", () => {
    expect(describeUpdateState(makeStatus("available", { info }), copy)).toBe("available 0.2.0");
    expect(describeUpdateState(makeStatus("downloaded", { info }), copy)).toBe("downloaded 0.2.0");
  });

  test("reports clamped download progress", () => {
    expect(describeUpdateState(makeStatus("downloading", { info, progress: makeProgress({ percent: 41.6 }) }), copy)).toBe("downloading 42");
    expect(describeUpdateState(makeStatus("downloading", { info }), copy)).toBe("downloading 0");
  });

  test.each([
    ["checking", "checking"],
    ["error", "error"]
  ] as const)("%s maps to its copy", (state, expected) => {
    expect(describeUpdateState(makeStatus(state), copy)).toBe(expected);
  });
});
