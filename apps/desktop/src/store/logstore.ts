import { useSyncExternalStore } from "react";
import type { LauncherLogEntry } from "../../shared/types/events";

// Modded Minecraft dumps thousands of spammy log lines per second on startup.
// Keep it in a raw ring buffer and throttle UI updates to rAF so React doesn't choke.
export class LogStore {
  private entries: LauncherLogEntry[] = [];
  private readonly listeners = new Set<() => void>();
  private frame: number | null = null;

  constructor(
    private readonly capacity = 5000,
    private readonly schedule: (callback: () => void) => number = defaultSchedule
  ) {}

  snapshot = (): readonly LauncherLogEntry[] => this.entries;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  append(batch: readonly LauncherLogEntry[]): void {
    if (batch.length === 0) {
      return;
    }

    const merged = this.entries.concat(batch);
    this.entries = merged.length > this.capacity ? merged.slice(merged.length - this.capacity) : merged;
    this.notify();
  }

  replace(entries: readonly LauncherLogEntry[]): void {
    this.entries = entries.slice(-this.capacity);
    this.notify();
  }

  clear(): void {
    this.entries = [];
    this.notify();
  }

  private notify(): void {
    if (this.frame !== null) {
      return;
    }

    this.frame = this.schedule(() => {
      this.frame = null;
      this.listeners.forEach((listener) => listener());
    });
  }
}

function defaultSchedule(callback: () => void): number {
  return typeof requestAnimationFrame === "function" ? requestAnimationFrame(callback) : Number(setTimeout(callback, 16));
}

export const logStore = new LogStore();

export function useLogEntries(store: LogStore = logStore): readonly LauncherLogEntry[] {
  return useSyncExternalStore(store.subscribe, store.snapshot);
}
