export type MinecraftVersionType = "release" | "snapshot" | "old_beta" | "old_alpha";

export interface MinecraftVersion {
  id: string;
  type: MinecraftVersionType;
  url: string;
  time: string;
  releaseTime: string;
}

export interface MinecraftVersionManifest {
  latest: {
    release: string;
    snapshot: string;
  };
  versions: MinecraftVersion[];
}
