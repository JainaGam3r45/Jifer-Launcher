import { describe, expect, test } from "bun:test";
import {
  buildPatch,
  draftFromInstance,
  isDraftDirty,
  validateDraft,
  validateInstanceName,
  validateNewInstance,
  type InstanceDraft
} from "../src/store/instancedraft";
import type { Instance } from "../shared/types/instance";

const globalWindow = { width: 1280, height: 720, fullscreen: false };

const instance: Instance = {
  schemaVersion: 1,
  id: "survival",
  name: "Survival",
  versionId: "1.20.1",
  loaderType: "vanilla",
  javaPath: "",
  memoryConfig: { min: "2G", max: "4G" },
  icon: "grass",
  gameDir: "/instances/survival/minecraft",
  createdAt: "2026-01-01T00:00:00.000Z",
  lastPlayedAt: null
};

const base = draftFromInstance(instance, globalWindow);
const draft = (overrides: Partial<InstanceDraft>): InstanceDraft => ({ ...base, ...overrides });

describe("draftFromInstance", () => {
  test("uses the global window when the instance has no override", () => {
    expect(base).toMatchObject({ useGlobalWindow: true, width: "1280", height: "720", fullscreen: false });
  });

  test("uses the instance window when it overrides the global one", () => {
    const custom = draftFromInstance({ ...instance, window: { width: 1920, height: 1080, fullscreen: true } }, globalWindow);

    expect(custom).toMatchObject({ useGlobalWindow: false, width: "1920", height: "1080", fullscreen: true });
  });
});

describe("validateDraft", () => {
  test("accepts the untouched draft", () => {
    expect(validateDraft(base)).toEqual({});
  });

  test.each([
    ["an empty name", { name: "   " }, { name: "nameRequired" }],
    ["a name over 64 characters", { name: "x".repeat(65) }, { name: "nameTooLong" }],
    ["a bare number as min memory", { memoryMin: "2" }, { memoryMin: "memoryMinFormat" }],
    ["a zero-led max memory", { memoryMax: "02G" }, { memoryMax: "memoryMaxFormat" }],
    ["a GB suffix", { memoryMax: "4GB" }, { memoryMax: "memoryMaxFormat" }],
    ["min above max", { memoryMin: "8G", memoryMax: "4G" }, { memoryMax: "memoryOrder" }],
    ["min above max across units", { memoryMin: "3G", memoryMax: "2048M" }, { memoryMax: "memoryOrder" }],
    ["a Java path with a null byte", { javaPath: "java\0" }, { javaPath: "javaPathInvalid" }]
  ])("flags %s", (_label, overrides, expected) => {
    expect(validateDraft(draft(overrides))).toEqual(expected);
  });

  test("accepts lowercase units and equal min and max", () => {
    expect(validateDraft(draft({ memoryMin: "2g", memoryMax: "2G" }))).toEqual({});
  });

  test("ignores the window fields while the global window is used", () => {
    expect(validateDraft(draft({ useGlobalWindow: true, width: "abc", height: "" }))).toEqual({});
  });

  test.each([
    ["319", "widthRange"],
    ["7681", "widthRange"],
    ["12.5", "widthRange"],
    ["", "widthRange"],
    ["-1280", "widthRange"]
  ])("rejects width %j", (width, code) => {
    expect(validateDraft(draft({ useGlobalWindow: false, width }))).toEqual({ width: code });
  });

  test.each([
    ["239", "heightRange"],
    ["4321", "heightRange"],
    ["abc", "heightRange"]
  ])("rejects height %j", (height, code) => {
    expect(validateDraft(draft({ useGlobalWindow: false, height }))).toEqual({ height: code });
  });

  test("accepts the window limits", () => {
    expect(validateDraft(draft({ useGlobalWindow: false, width: "320", height: "240" }))).toEqual({});
    expect(validateDraft(draft({ useGlobalWindow: false, width: "7680", height: "4320" }))).toEqual({});
  });
});

describe("buildPatch", () => {
  test("normalizes text and sends null to fall back to the global window", () => {
    const patch = buildPatch(draft({ name: "  Renamed ", memoryMin: "1g", memoryMax: " 6G ", javaPath: " C:\\Java\\javaw.exe " }));

    expect(patch).toEqual({
      name: "Renamed",
      versionId: "1.20.1",
      memoryConfig: { min: "1G", max: "6G" },
      javaPath: "C:\\Java\\javaw.exe",
      window: null
    });
  });

  test("sends numeric window values when the override is on", () => {
    expect(buildPatch(draft({ useGlobalWindow: false, width: "1920", height: "1080", fullscreen: true })).window).toEqual({
      width: 1920,
      height: 1080,
      fullscreen: true
    });
  });
});

describe("isDraftDirty", () => {
  test("is clean for the base draft and for cosmetic whitespace or case", () => {
    expect(isDraftDirty(base, base)).toBe(false);
    expect(isDraftDirty(draft({ name: " Survival ", memoryMax: "4g" }), base)).toBe(false);
  });

  test("detects edits", () => {
    expect(isDraftDirty(draft({ name: "Other" }), base)).toBe(true);
    expect(isDraftDirty(draft({ versionId: "1.21" }), base)).toBe(true);
    expect(isDraftDirty(draft({ memoryMax: "8G" }), base)).toBe(true);
    expect(isDraftDirty(draft({ useGlobalWindow: false }), base)).toBe(true);
  });

  test("ignores window edits while the global window is used", () => {
    expect(isDraftDirty(draft({ width: "1920", fullscreen: true }), base)).toBe(false);
  });
});

describe("validateNewInstance", () => {
  test("requires a name and a version", () => {
    expect(validateNewInstance({ name: " ", versionId: "" })).toEqual({ name: "nameRequired", versionId: "versionRequired" });
    expect(validateNewInstance({ name: "x".repeat(65), versionId: "1.20.1" })).toEqual({ name: "nameTooLong" });
    expect(validateNewInstance({ name: "Survival", versionId: "1.20.1" })).toEqual({});
  });
});

describe("validateInstanceName", () => {
  test("returns the first problem or null", () => {
    expect(validateInstanceName("  ")).toBe("nameRequired");
    expect(validateInstanceName("x".repeat(65))).toBe("nameTooLong");
    expect(validateInstanceName("x".repeat(64))).toBeNull();
    expect(validateInstanceName(" Survival ")).toBeNull();
  });
});
