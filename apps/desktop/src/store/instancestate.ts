import type { LaunchStateEvent, LauncherState } from "../../shared/types/events";
import type { Instance } from "../../shared/types/instance";

const defaultRecentLimit = 5;
const busyStates: readonly LauncherState[] = ["installing", "launching", "running", "stopping"];

export function upsertInstance(list: Instance[], instance: Instance): Instance[] {
  const exists = list.some((item) => item.id === instance.id);

  return exists ? list.map((item) => (item.id === instance.id ? instance : item)) : [...list, instance];
}

export function removeInstance(list: Instance[], id: string): Instance[] {
  return list.filter((item) => item.id !== id);
}

export function sortInstances(list: Instance[]): Instance[] {
  return [...list].sort((a, b) => {
    const playedA = toTime(a.lastPlayedAt);
    const playedB = toTime(b.lastPlayedAt);

    if (playedA !== playedB) {
      return playedB - playedA;
    }

    return toTime(b.createdAt) - toTime(a.createdAt) || a.name.localeCompare(b.name) || a.id.localeCompare(b.id);
  });
}

export function resolveFallbackActiveId(list: Instance[], removedId: string): string | null {
  return sortInstances(removeInstance(list, removedId))[0]?.id ?? null;
}

export function deriveRecentVersions(list: Instance[], limit = defaultRecentLimit): string[] {
  const versions = new Set<string>();

  for (const instance of sortInstances(list)) {
    if (instance.lastPlayedAt === null) {
      break;
    }

    if (instance.versionId) {
      versions.add(instance.versionId);
    }
  }

  return [...versions].slice(0, limit);
}

function toTime(value: string | null): number {
  if (value === null) {
    return Number.NEGATIVE_INFINITY;
  }

  const time = Date.parse(value);
  return Number.isNaN(time) ? Number.NEGATIVE_INFINITY : time;
}

export function pickQuickInstances(list: Instance[], activeId: string | null, limit = defaultRecentLimit): Instance[] {
  const top = list.slice(0, limit);
  const active = activeId ? list.find((item) => item.id === activeId) : undefined;

  if (!active || top.includes(active)) {
    return top;
  }

  return [active, ...top.slice(0, limit - 1)];
}

export interface InstanceLaunchView {
  state: LauncherState;
  blocked: boolean;
  runningInstanceId: string | null;
}

export function resolveInstanceLaunchView(status: LaunchStateEvent, activeId: string | null): InstanceLaunchView {
  if (!busyStates.includes(status.state)) {
    return { state: "idle", blocked: false, runningInstanceId: null };
  }

  const ownerId = status.instanceId ?? activeId;

  if (ownerId === activeId) {
    return { state: status.state, blocked: false, runningInstanceId: ownerId };
  }

  return { state: "idle", blocked: true, runningInstanceId: ownerId };
}
