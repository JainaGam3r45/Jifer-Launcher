import { describe, expect, test } from "bun:test";
import type { DownloadTelemetry } from "../shared/types/events";
import { downloadPercent, formatDuration, phaseStates, remainingSeconds } from "../src/store/telemetry";

function telemetry(overrides: Partial<DownloadTelemetry> = {}): DownloadTelemetry {
  return {
    filesTotal: 0,
    filesDone: 0,
    filesReused: 0,
    filesActive: 0,
    filesVerifying: 0,
    filesVerified: 0,
    retries: 0,
    bytesTotal: 0,
    bytesDone: 0,
    bytesPerSecond: 0,
    ...overrides
  };
}

describe("phaseStates", () => {
  test("marks earlier phases done and later ones pending", () => {
    expect(phaseStates("natives")).toEqual({ metadata: "done", files: "done", natives: "active", java: "pending", starting: "pending" });
  });

  test("leaves everything pending before the first event", () => {
    expect(Object.values(phaseStates(null))).toEqual(["pending", "pending", "pending", "pending", "pending"]);
  });
});

describe("downloadPercent", () => {
  test("prefers bytes, falls back to files and clamps", () => {
    expect(downloadPercent(telemetry({ bytesTotal: 200, bytesDone: 50, filesTotal: 10, filesDone: 9 }))).toBe(25);
    expect(downloadPercent(telemetry({ filesTotal: 4, filesDone: 1 }))).toBe(25);
    expect(downloadPercent(telemetry({ bytesTotal: 100, bytesDone: 150 }))).toBe(100);
  });

  test("is unknown without data", () => {
    expect(downloadPercent(undefined)).toBeNull();
    expect(downloadPercent(telemetry())).toBeNull();
  });
});

describe("remainingSeconds", () => {
  test("divides what is left by the current speed", () => {
    expect(remainingSeconds(telemetry({ bytesTotal: 1000, bytesDone: 400, bytesPerSecond: 100 }))).toBe(6);
  });

  test("is unknown while stalled or unsized", () => {
    expect(remainingSeconds(telemetry({ bytesTotal: 1000, bytesDone: 400 }))).toBeNull();
    expect(remainingSeconds(telemetry({ bytesPerSecond: 100 }))).toBeNull();
  });
});

describe("formatDuration", () => {
  test("scales units", () => {
    expect(formatDuration(42, "en")).toBe("42s");
    expect(formatDuration(120, "en")).toBe("2m");
    expect(formatDuration(125, "en")).toBe("2m 5s");
    expect(formatDuration(3720, "en")).toBe("1h 2m");
  });
});
