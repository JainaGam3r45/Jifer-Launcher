export const loaderTypes = ["vanilla", "forge", "fabric", "quilt", "neoforge"] as const;

export type LoaderType = (typeof loaderTypes)[number];

export interface MemoryConfig {
  min: string;
  max: string;
}

export interface WindowConfig {
  width: number;
  height: number;
  fullscreen: boolean;
}

export interface Instance {
  schemaVersion: 1;
  id: string;
  name: string;
  versionId: string;
  loaderType: LoaderType;
  loaderVersion?: string;
  javaPath: string;
  memoryConfig: MemoryConfig;
  window?: WindowConfig;
  icon: string;
  gameDir: string;
  createdAt: string;
  lastPlayedAt: string | null;
}

export interface CreateInstanceInput {
  name: string;
  versionId: string;
  loaderType: LoaderType;
  loaderVersion?: string;
  javaPath?: string;
  memoryConfig?: MemoryConfig;
  window?: WindowConfig;
  icon?: string;
}

export type UpdateInstanceInput = Partial<Omit<Instance, "schemaVersion" | "id" | "gameDir" | "createdAt" | "window">> & {
  window?: WindowConfig | null;
};

export interface CloneInstanceInput {
  sourceId: string;
  name: string;
  copyWorlds: boolean;
}
