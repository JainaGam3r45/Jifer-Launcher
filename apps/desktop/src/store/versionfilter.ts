import type { MinecraftVersion } from "../../shared/types/version";

export type VersionFilter = "release" | "snapshot" | "all";

const defaultLimit = 180;

export function filterVersions(
  versions: MinecraftVersion[],
  filter: VersionFilter,
  query: string,
  selectedId = "",
  limit = defaultLimit
): MinecraftVersion[] {
  const needle = query.trim().toLowerCase();
  const matches = versions
    .filter((version) => filter === "all" || version.type === filter)
    .filter((version) => !needle || version.id.toLowerCase().includes(needle))
    .slice(0, limit);

  if (!selectedId || matches.some((version) => version.id === selectedId)) {
    return matches;
  }

  const selected = versions.find((version) => version.id === selectedId);
  return selected ? [selected, ...matches] : matches;
}
