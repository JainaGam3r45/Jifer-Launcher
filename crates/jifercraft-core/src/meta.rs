//! Mojang launcher metadata: the version manifest, per-version JSON and asset indexes.
//!
//! Every file is cached under the configured Minecraft root (not the default one), verified by SHA-1 when
//! Mojang publishes a hash, and reused when the network is down.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jifercraft_net::{Checksum, Transport, Url, fetch_bytes};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::{CoreError, CoreResult, blocking};
use crate::store;
use crate::version::{AssetIndex, AssetIndexRef, VersionJson};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const METADATA_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    /// `release`, `snapshot`, `old_beta` or `old_alpha`.
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
}

#[derive(Debug)]
pub struct MetadataClient<T> {
    transport: Arc<T>,
    manifest_url: Url,
    cache_file: PathBuf,
    manifest: Mutex<Option<VersionManifest>>,
}

impl<T: Transport> MetadataClient<T> {
    pub fn new(transport: Arc<T>, cache_dir: &Path) -> CoreResult<Self> {
        let manifest_url = parse_url("version manifest URL", MANIFEST_URL)?;
        Ok(Self {
            transport,
            manifest_url,
            cache_file: cache_dir.join("version_manifest_v2.json"),
            manifest: Mutex::new(None),
        })
    }

    #[must_use]
    pub fn with_manifest_url(mut self, url: Url) -> Self {
        self.manifest_url = url;
        self
    }

    /// The manifest, fetched once per process. Falls back to the last cached copy when offline.
    pub async fn manifest(&self) -> CoreResult<VersionManifest> {
        let mut cached = self.manifest.lock().await;
        if let Some(manifest) = cached.as_ref() {
            return Ok(manifest.clone());
        }
        let manifest = match self.fetch_manifest().await {
            Ok(manifest) => manifest,
            Err(error) => {
                let cache_file = self.cache_file.clone();
                let stored = blocking(move || store::read_json::<VersionManifest>(&cache_file))
                    .await
                    .ok()
                    .flatten();
                match stored {
                    Some(manifest) => {
                        tracing::warn!(%error, "version manifest unavailable; using the cached copy");
                        manifest
                    }
                    None => return Err(error),
                }
            }
        };
        *cached = Some(manifest.clone());
        Ok(manifest)
    }

    /// The version JSON, cached at `<root>/versions/<id>/<id>.json`.
    pub async fn version(&self, minecraft_root: &Path, id: &str) -> CoreResult<VersionJson> {
        let dest = minecraft_root.join("versions").join(id).join(format!("{id}.json"));
        let bytes = match self.manifest().await {
            Ok(manifest) => {
                let entry = manifest
                    .versions
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| CoreError::VersionNotFound(id.to_owned()))?;
                let checksum = entry.sha1.as_deref().map(Checksum::sha1_hex).transpose()?;
                self.cached_or_fetch(&parse_url("version URL", &entry.url)?, checksum, dest)
                    .await?
            }
            // Offline with no manifest: an already installed version can still be launched.
            Err(error) => read_existing(dest).await?.ok_or(error)?,
        };
        parse(&format!("version {id}"), &bytes)
    }

    /// The asset index, cached at `<root>/assets/indexes/<id>.json`.
    pub async fn asset_index(&self, minecraft_root: &Path, index: &AssetIndexRef) -> CoreResult<AssetIndex> {
        let dest = minecraft_root
            .join("assets")
            .join("indexes")
            .join(format!("{}.json", index.id));
        let checksum = Checksum::sha1_hex(&index.sha1)?;
        let bytes = self
            .cached_or_fetch(&parse_url("asset index URL", &index.url)?, Some(checksum), dest)
            .await?;
        parse(&format!("asset index {}", index.id), &bytes)
    }

    async fn fetch_manifest(&self) -> CoreResult<VersionManifest> {
        let bytes = fetch_bytes(self.transport.as_ref(), &self.manifest_url, METADATA_LIMIT).await?;
        let manifest: VersionManifest = parse("version manifest", &bytes)?;
        let (cache_file, copy) = (self.cache_file.clone(), manifest.clone());
        if let Err(error) = blocking(move || store::write_json_atomic(&cache_file, &copy)).await {
            tracing::warn!(%error, "could not cache the version manifest");
        }
        Ok(manifest)
    }

    async fn cached_or_fetch(&self, url: &Url, checksum: Option<Checksum>, dest: PathBuf) -> CoreResult<Vec<u8>> {
        if let Some(bytes) = read_existing(dest.clone()).await? {
            if checksum.is_none_or(|expected| hash(&bytes, expected) == expected) {
                return Ok(bytes);
            }
            tracing::info!(path = %dest.display(), "cached metadata is stale or corrupt; downloading it again");
        }
        let bytes = fetch_bytes(self.transport.as_ref(), url, METADATA_LIMIT).await?;
        if let Some(expected) = checksum {
            let actual = hash(&bytes, expected);
            if actual != expected {
                return Err(jifercraft_net::NetError::ChecksumMismatch {
                    url: url.to_string(),
                    expected: expected.to_hex(),
                    actual: actual.to_hex(),
                }
                .into());
            }
        }
        let body = bytes.clone();
        blocking(move || store::write_atomic(&dest, &body)).await?;
        Ok(bytes)
    }
}

async fn read_existing(path: PathBuf) -> CoreResult<Option<Vec<u8>>> {
    blocking(move || match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::io(path, error)),
    })
    .await
}

fn hash(bytes: &[u8], like: Checksum) -> Checksum {
    let mut hasher = like.hasher();
    hasher.update(bytes);
    hasher.finish()
}

fn parse<D: serde::de::DeserializeOwned>(what: &str, bytes: &[u8]) -> CoreResult<D> {
    serde_json::from_slice(bytes).map_err(|error| CoreError::Metadata {
        what: what.to_owned(),
        reason: error.to_string(),
    })
}

pub(crate) fn parse_url(what: &str, raw: &str) -> CoreResult<Url> {
    Url::parse(raw).map_err(|error| CoreError::Metadata {
        what: what.to_owned(),
        reason: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{MemoryTransport, sha1_hex};

    const VERSION_JSON: &str = r#"{"id":"1.20.1","type":"release","mainClass":"net.minecraft.client.main.Main"}"#;

    fn manifest_body(version_sha1: &str) -> String {
        format!(
            r#"{{"latest":{{"release":"1.20.1","snapshot":"23w31a"}},"versions":[
                {{"id":"1.20.1","type":"release","url":"https://meta.test/1.20.1.json","time":"t","releaseTime":"r","sha1":"{version_sha1}"}}]}}"#
        )
    }

    fn client(transport: &Arc<MemoryTransport>, cache: &Path) -> MetadataClient<MemoryTransport> {
        MetadataClient::new(Arc::clone(transport), cache)
            .unwrap()
            .with_manifest_url(Url::parse("https://meta.test/manifest.json").unwrap())
    }

    #[tokio::test]
    async fn fetches_caches_and_reuses_versions() {
        let dir = tempfile::tempdir().unwrap();
        let transport = MemoryTransport::new();
        transport.serve(
            "https://meta.test/manifest.json",
            manifest_body(&sha1_hex(VERSION_JSON.as_bytes())),
        );
        transport.serve("https://meta.test/1.20.1.json", VERSION_JSON);
        let meta = client(&transport, dir.path());

        let manifest = meta.manifest().await.unwrap();
        assert_eq!(manifest.latest.release, "1.20.1");
        let version = meta.version(&dir.path().join("mc"), "1.20.1").await.unwrap();
        assert_eq!(version.main_class, "net.minecraft.client.main.Main");
        assert!(dir.path().join("mc/versions/1.20.1/1.20.1.json").is_file());

        meta.version(&dir.path().join("mc"), "1.20.1").await.unwrap();
        assert_eq!(transport.hits("https://meta.test/manifest.json"), 1);
        assert_eq!(transport.hits("https://meta.test/1.20.1.json"), 1);
        assert_eq!(
            meta.version(&dir.path().join("mc"), "9.9").await.unwrap_err().code(),
            "version_not_found"
        );
    }

    #[tokio::test]
    async fn rejects_tampered_metadata_and_repairs_a_corrupt_cache() {
        let dir = tempfile::tempdir().unwrap();
        let transport = MemoryTransport::new();
        transport.serve(
            "https://meta.test/manifest.json",
            manifest_body(&sha1_hex(VERSION_JSON.as_bytes())),
        );
        transport.serve("https://meta.test/1.20.1.json", VERSION_JSON.replace("Main", "Evil"));
        let meta = client(&transport, dir.path());
        assert_eq!(
            meta.version(dir.path(), "1.20.1").await.unwrap_err().code(),
            "checksum_mismatch"
        );

        transport.serve("https://meta.test/1.20.1.json", VERSION_JSON);
        let cached = dir.path().join("versions/1.20.1/1.20.1.json");
        std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
        std::fs::write(&cached, "{ corrupt").unwrap();
        meta.version(dir.path(), "1.20.1").await.unwrap();
        assert_eq!(std::fs::read_to_string(cached).unwrap(), VERSION_JSON);
    }

    #[tokio::test]
    async fn works_offline_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let online = MemoryTransport::new();
        online.serve(
            "https://meta.test/manifest.json",
            manifest_body(&sha1_hex(VERSION_JSON.as_bytes())),
        );
        online.serve("https://meta.test/1.20.1.json", VERSION_JSON);
        client(&online, dir.path()).version(dir.path(), "1.20.1").await.unwrap();

        let offline = client(&MemoryTransport::new(), dir.path());
        assert_eq!(offline.manifest().await.unwrap().latest.release, "1.20.1");
        assert_eq!(offline.version(dir.path(), "1.20.1").await.unwrap().id, "1.20.1");
    }
}
