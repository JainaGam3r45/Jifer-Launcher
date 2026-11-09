import { describe, expect, test } from "bun:test";
import { formatError } from "../src/store/errors";
import { filterVersions } from "../src/store/versionfilter";
import type { MinecraftVersion, MinecraftVersionType } from "../shared/types/version";

function makeVersion(id: string, type: MinecraftVersionType): MinecraftVersion {
  return { id, type, url: `https://example.test/${id}.json`, time: "", releaseTime: "" };
}

const versions = [
  makeVersion("1.21", "release"),
  makeVersion("24w14a", "snapshot"),
  makeVersion("1.20.1", "release"),
  makeVersion("b1.7.3", "old_beta")
];

const ids = (list: MinecraftVersion[]): string[] => list.map((version) => version.id);

describe("filterVersions", () => {
  test("filters by type", () => {
    expect(ids(filterVersions(versions, "release", ""))).toEqual(["1.21", "1.20.1"]);
    expect(ids(filterVersions(versions, "snapshot", ""))).toEqual(["24w14a"]);
    expect(ids(filterVersions(versions, "all", ""))).toEqual(["1.21", "24w14a", "1.20.1", "b1.7.3"]);
  });

  test("searches case-insensitively and trims the query", () => {
    expect(ids(filterVersions(versions, "all", "  24W "))).toEqual(["24w14a"]);
    expect(ids(filterVersions(versions, "release", "1.20"))).toEqual(["1.20.1"]);
  });

  test("keeps the selected version visible even when it does not match", () => {
    expect(ids(filterVersions(versions, "release", "1.21", "b1.7.3"))).toEqual(["b1.7.3", "1.21"]);
    expect(ids(filterVersions(versions, "release", "", "1.21"))).toEqual(["1.21", "1.20.1"]);
  });

  test("ignores an unknown selected version and applies the limit", () => {
    expect(ids(filterVersions(versions, "all", "", "missing"))).toHaveLength(4);
    expect(ids(filterVersions(versions, "all", "", "", 2))).toEqual(["1.21", "24w14a"]);
  });
});

describe("formatError", () => {
  test("translates backend error codes and falls back to the backend message", () => {
    const error = { code: "instance_active", message: "Select another instance before deleting this one." };
    expect(formatError(error, { instance_active: "Elige otra instancia antes de borrar esta." })).toBe("Elige otra instancia antes de borrar esta.");
    expect(formatError(error)).toBe("Select another instance before deleting this one.");
  });

  test("leaves plain messages and non-errors alone", () => {
    expect(formatError(new Error("Plain failure"))).toBe("Plain failure");
    expect(formatError("text")).toBe("text");
    expect(formatError({ code: 42 })).toBe("[object Object]");
  });
});
