import { describe, expect, test } from "bun:test";
import { formatRelativeTime, loaderLabel } from "../src/format";

const now = Date.parse("2026-06-15T12:00:00.000Z");
const ago = (milliseconds: number): string => new Date(now - milliseconds).toISOString();
const minute = 60_000;
const hour = 60 * minute;
const day = 24 * hour;

describe("formatRelativeTime", () => {
  test("describes recent activity in English", () => {
    expect(formatRelativeTime(ago(10_000), now, "en")).toBe("now");
    expect(formatRelativeTime(ago(5 * minute), now, "en")).toBe("5 minutes ago");
    expect(formatRelativeTime(ago(3 * hour), now, "en")).toBe("3 hours ago");
    expect(formatRelativeTime(ago(day), now, "en")).toBe("yesterday");
    expect(formatRelativeTime(ago(4 * day), now, "en")).toBe("4 days ago");
  });

  test("describes recent activity in Spanish", () => {
    expect(formatRelativeTime(ago(2 * hour), now, "es")).toBe("hace 2 horas");
    expect(formatRelativeTime(ago(day), now, "es")).toBe("ayer");
  });

  test("falls back to a calendar date after a month", () => {
    const formatted = formatRelativeTime(ago(45 * day), now, "en");

    expect(formatted).toContain("2026");
    expect(formatted).not.toContain("ago");
  });

  test("never reports the future and tolerates invalid input", () => {
    expect(formatRelativeTime(new Date(now + 5 * minute).toISOString(), now, "en")).toBe("now");
    expect(formatRelativeTime("not a date", now, "en")).toBe("");
  });
});

describe("loaderLabel", () => {
  test("returns display names", () => {
    expect(loaderLabel("vanilla")).toBe("Vanilla");
    expect(loaderLabel("neoforge")).toBe("NeoForge");
  });
});
