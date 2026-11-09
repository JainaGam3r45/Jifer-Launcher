import { describe, expect, test } from "bun:test";
import {
  deriveRecentVersions,
  pickQuickInstances,
  removeInstance,
  resolveFallbackActiveId,
  resolveInstanceLaunchView,
  sortInstances,
  upsertInstance
} from "../src/store/instancestate";
import type { LaunchStateEvent } from "../shared/types/events";
import type { Instance } from "../shared/types/instance";

function makeInstance(id: string, overrides: Partial<Instance> = {}): Instance {
  return {
    schemaVersion: 1,
    id,
    name: id,
    versionId: "1.20.1",
    loaderType: "vanilla",
    javaPath: "",
    memoryConfig: { min: "2G", max: "4G" },
    icon: "grass",
    gameDir: `/instances/${id}/minecraft`,
    createdAt: "2026-01-01T00:00:00.000Z",
    lastPlayedAt: null,
    ...overrides
  };
}

const ids = (list: Instance[]): string[] => list.map((item) => item.id);

describe("sortInstances", () => {
  test("puts the most recently played first", () => {
    const list = [
      makeInstance("old", { lastPlayedAt: "2026-02-01T00:00:00.000Z" }),
      makeInstance("new", { lastPlayedAt: "2026-03-01T00:00:00.000Z" }),
      makeInstance("mid", { lastPlayedAt: "2026-02-15T00:00:00.000Z" })
    ];

    expect(ids(sortInstances(list))).toEqual(["new", "mid", "old"]);
  });

  test("lists played instances before never-played ones, which go newest created first", () => {
    const list = [
      makeInstance("unplayed-old", { createdAt: "2026-01-01T00:00:00.000Z" }),
      makeInstance("unplayed-new", { createdAt: "2026-04-01T00:00:00.000Z" }),
      makeInstance("played", { lastPlayedAt: "2026-02-01T00:00:00.000Z", createdAt: "2025-01-01T00:00:00.000Z" })
    ];

    expect(ids(sortInstances(list))).toEqual(["played", "unplayed-new", "unplayed-old"]);
  });

  test("breaks ties by creation date, then name, then id", () => {
    const played = "2026-02-01T00:00:00.000Z";
    const list = [
      makeInstance("c", { name: "Same", lastPlayedAt: played, createdAt: "2026-01-02T00:00:00.000Z" }),
      makeInstance("b", { name: "Same", lastPlayedAt: played, createdAt: "2026-01-01T00:00:00.000Z" }),
      makeInstance("a", { name: "Same", lastPlayedAt: played, createdAt: "2026-01-01T00:00:00.000Z" }),
      makeInstance("z", { name: "Alpha", lastPlayedAt: played, createdAt: "2026-01-01T00:00:00.000Z" })
    ];

    expect(ids(sortInstances(list))).toEqual(["c", "z", "a", "b"]);
  });

  test("treats unparseable timestamps as never played and does not mutate the input", () => {
    const list = [makeInstance("broken", { lastPlayedAt: "not a date" }), makeInstance("ok", { lastPlayedAt: "2026-02-01T00:00:00.000Z" })];
    const snapshot = ids(list);

    expect(ids(sortInstances(list))).toEqual(["ok", "broken"]);
    expect(ids(list)).toEqual(snapshot);
  });
});

describe("upsertInstance", () => {
  test("appends an unknown instance without touching the others", () => {
    const list = [makeInstance("a")];
    const next = upsertInstance(list, makeInstance("b"));

    expect(ids(next)).toEqual(["a", "b"]);
    expect(next[0]).toBe(list[0]);
  });

  test("replaces an existing instance in place", () => {
    const list = [makeInstance("a"), makeInstance("b"), makeInstance("c")];
    const updated = makeInstance("b", { name: "Renamed" });

    const next = upsertInstance(list, updated);

    expect(ids(next)).toEqual(["a", "b", "c"]);
    expect(next[1]).toBe(updated);
    expect(list[1]?.name).toBe("b");
  });
});

describe("removeInstance", () => {
  test("drops the matching instance and ignores unknown ids", () => {
    const list = [makeInstance("a"), makeInstance("b")];

    expect(ids(removeInstance(list, "a"))).toEqual(["b"]);
    expect(ids(removeInstance(list, "missing"))).toEqual(["a", "b"]);
    expect(ids(list)).toEqual(["a", "b"]);
  });
});

describe("resolveFallbackActiveId", () => {
  test("picks the most recently played remaining instance", () => {
    const list = [
      makeInstance("removed", { lastPlayedAt: "2026-05-01T00:00:00.000Z" }),
      makeInstance("recent", { lastPlayedAt: "2026-03-01T00:00:00.000Z" }),
      makeInstance("older", { lastPlayedAt: "2026-02-01T00:00:00.000Z" })
    ];

    expect(resolveFallbackActiveId(list, "removed")).toBe("recent");
  });

  test("works whether or not the removed instance is still in the list", () => {
    const remaining = [makeInstance("only")];

    expect(resolveFallbackActiveId(remaining, "gone")).toBe("only");
    expect(resolveFallbackActiveId([...remaining, makeInstance("gone")], "gone")).toBe("only");
  });

  test("returns null when nothing else is left", () => {
    expect(resolveFallbackActiveId([makeInstance("last")], "last")).toBeNull();
    expect(resolveFallbackActiveId([], "any")).toBeNull();
  });
});

describe("deriveRecentVersions", () => {
  test("lists unique versions of played instances, most recent first", () => {
    const list = [
      makeInstance("a", { versionId: "1.20.1", lastPlayedAt: "2026-03-01T00:00:00.000Z" }),
      makeInstance("b", { versionId: "1.21", lastPlayedAt: "2026-04-01T00:00:00.000Z" }),
      makeInstance("c", { versionId: "1.20.1", lastPlayedAt: "2026-02-01T00:00:00.000Z" }),
      makeInstance("d", { versionId: "1.8.9", lastPlayedAt: "2026-01-01T00:00:00.000Z" })
    ];

    expect(deriveRecentVersions(list)).toEqual(["1.21", "1.20.1", "1.8.9"]);
  });

  test("ignores instances that were never played or have no version", () => {
    const list = [
      makeInstance("unplayed", { versionId: "1.19" }),
      makeInstance("blank", { versionId: "", lastPlayedAt: "2026-03-01T00:00:00.000Z" }),
      makeInstance("played", { versionId: "1.20.1", lastPlayedAt: "2026-02-01T00:00:00.000Z" })
    ];

    expect(deriveRecentVersions(list)).toEqual(["1.20.1"]);
  });

  test("caps the result at the limit and handles an empty list", () => {
    const list = Array.from({ length: 8 }, (_, index) =>
      makeInstance(`i${index}`, { versionId: `1.${index}`, lastPlayedAt: `2026-01-0${index + 1}T00:00:00.000Z` })
    );

    expect(deriveRecentVersions(list)).toHaveLength(5);
    expect(deriveRecentVersions(list, 2)).toEqual(["1.7", "1.6"]);
    expect(deriveRecentVersions([])).toEqual([]);
  });
});

describe("pickQuickInstances", () => {
  const many = Array.from({ length: 8 }, (_, index) => makeInstance(`i${index}`));

  test("returns at most the first five instances", () => {
    expect(ids(pickQuickInstances(many, "i0"))).toEqual(["i0", "i1", "i2", "i3", "i4"]);
    expect(pickQuickInstances(many.slice(0, 2), null)).toHaveLength(2);
  });

  test("pulls the active instance into the list when it would fall outside", () => {
    expect(ids(pickQuickInstances(many, "i7"))).toEqual(["i7", "i0", "i1", "i2", "i3"]);
  });

  test("ignores an unknown active id", () => {
    expect(ids(pickQuickInstances(many, "missing"))).toEqual(["i0", "i1", "i2", "i3", "i4"]);
    expect(pickQuickInstances([], "any")).toEqual([]);
  });
});

describe("resolveInstanceLaunchView", () => {
  const event = (state: LaunchStateEvent["state"], instanceId?: string): LaunchStateEvent => ({ state, message: "", instanceId });

  test("is idle for idle and error states", () => {
    expect(resolveInstanceLaunchView(event("idle"), "a")).toEqual({ state: "idle", blocked: false, runningInstanceId: null });
    expect(resolveInstanceLaunchView(event("error"), "a")).toEqual({ state: "idle", blocked: false, runningInstanceId: null });
  });

  test("reports the state when the active instance owns it", () => {
    expect(resolveInstanceLaunchView(event("running", "a"), "a")).toEqual({ state: "running", blocked: false, runningInstanceId: "a" });
    expect(resolveInstanceLaunchView(event("stopping", "a"), "a").state).toBe("stopping");
  });

  test("treats events without an instance id as belonging to the active instance", () => {
    expect(resolveInstanceLaunchView(event("installing"), "a")).toEqual({ state: "installing", blocked: false, runningInstanceId: "a" });
  });

  test("blocks the active instance while another one runs", () => {
    expect(resolveInstanceLaunchView(event("running", "b"), "a")).toEqual({ state: "idle", blocked: true, runningInstanceId: "b" });
    expect(resolveInstanceLaunchView(event("launching", "b"), null).blocked).toBe(true);
  });
});
