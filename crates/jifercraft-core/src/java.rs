// Java runtime resolution:
// Minecraft's history with Java is a roller coaster:
// - 1.16 and older: Java 8 (ancient bedrock).
// - 1.17: Mojang briefly demanded Java 16, which died an early death.
// - 1.18 - 1.20.4: Java 17 LTS.
// - 1.20.5+: Java 21 LTS.
// We auto-fetch verified Eclipse Temurin JREs from Adoptium so the user never has
// to install 4 different JDKs on their machine manually.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jifercraft_net::{Checksum, DownloadJob, Downloader, ProgressFn, RetryPolicy, Transport, Url, fetch_bytes};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::error::{CoreError, CoreResult, IoContext, blocking};
use crate::fsutil;
use crate::meta::parse_url;
use crate::store;

pub const ADOPTIUM_API: &str = "https://api.adoptium.net/v3/";
// LTS majors that Adoptium keeps active. If a quirky version requests dead Java 16, bump to 17.
const PUBLISHED_MAJORS: [u32; 5] = [8, 11, 17, 21, 25];
const MANIFEST: &str = "runtime.json";

#[cfg(windows)]
const EXECUTABLE: &str = "javaw.exe";
#[cfg(not(windows))]
const EXECUTABLE: &str = "java";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntime {
    pub executable: PathBuf,
    /// `None` for user-provided or system runtimes, whose version is not inspected.
    pub major: Option<u32>,
    pub managed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledRuntime {
    major: u32,
    release: String,
    executable: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
    release_name: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    checksum: String,
    link: String,
    name: String,
    size: u64,
}

#[derive(Debug)]
pub struct RuntimeManager<T> {
    transport: Arc<T>,
    runtimes_dir: PathBuf,
    download_dir: PathBuf,
    api: Url,
    retry: RetryPolicy,
    install_lock: Mutex<()>,
}

impl<T: Transport> RuntimeManager<T> {
    pub fn new(transport: Arc<T>, runtimes_dir: PathBuf, download_dir: PathBuf) -> CoreResult<Self> {
        Ok(Self {
            transport,
            runtimes_dir,
            download_dir,
            api: parse_url("Adoptium API", ADOPTIUM_API)?,
            retry: RetryPolicy::default(),
            install_lock: Mutex::new(()),
        })
    }

    #[must_use]
    pub fn with_api(mut self, api: Url) -> Self {
        self.api = api;
        self
    }

    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Picks the runtime for a game that needs Java `major`.
    pub async fn resolve(
        &self,
        explicit_path: &str,
        major: u32,
        cancel: &CancellationToken,
        progress: ProgressFn,
    ) -> CoreResult<JavaRuntime> {
        if !explicit_path.trim().is_empty() {
            return explicit(explicit_path.trim());
        }
        if let Some(runtime) = self.installed(major).await? {
            return Ok(runtime);
        }
        if !cfg!(windows) {
            // Adoptium ships tar.gz outside Windows; until that is supported, defer to the system `java`.
            return Ok(JavaRuntime {
                executable: PathBuf::from("java"),
                major: None,
                managed: false,
            });
        }
        let _guard = self.install_lock.lock().await;
        if let Some(runtime) = self.installed(major).await? {
            return Ok(runtime);
        }
        let (published_major, asset) = match self.find_release(major).await {
            Ok(release) => release,
            Err(error) => {
                // Offline: a newer runtime already on disk beats failing outright.
                return match self.installed_newer(major).await? {
                    Some(runtime) => Ok(runtime),
                    None => Err(error),
                };
            }
        };
        // Unpublished majors (e.g. 16) resolve to the next one, which may already be installed.
        if published_major != major
            && let Some(runtime) = self.installed(published_major).await?
        {
            return Ok(runtime);
        }
        self.install(published_major, asset, cancel, progress).await
    }

    /// The managed runtime for exactly this major, if present and intact.
    async fn installed(&self, major: u32) -> CoreResult<Option<JavaRuntime>> {
        let dir = self.runtime_dir(major);
        blocking(move || Ok(read_runtime(&dir))).await
    }

    async fn installed_newer(&self, major: u32) -> CoreResult<Option<JavaRuntime>> {
        let dirs: Vec<PathBuf> = candidate_majors(major)
            .into_iter()
            .skip(1)
            .map(|candidate| self.runtime_dir(candidate))
            .collect();
        blocking(move || Ok(dirs.iter().find_map(|dir| read_runtime(dir)))).await
    }

    async fn install(
        &self,
        published_major: u32,
        asset: AdoptiumAsset,
        cancel: &CancellationToken,
        progress: ProgressFn,
    ) -> CoreResult<JavaRuntime> {
        let package = asset.binary.package;
        let archive = self.download_dir.join(file_name(&package.name)?);
        let job = DownloadJob {
            url: parse_url("Adoptium download", &package.link)?,
            dest: archive.clone(),
            size: Some(package.size),
            checksum: Some(Checksum::sha256_hex(&package.checksum)?),
        };
        Downloader::new(Arc::clone(&self.transport))
            .with_concurrency(1)
            .with_retry(self.retry)
            .run(vec![job], cancel, progress)
            .await?;

        let target = self.runtime_dir(published_major);
        let staging = self.runtimes_dir.join(format!(".temurin-{published_major}-staging"));
        let release = asset.release_name;
        let runtime = blocking(move || {
            let result = unpack(&archive, &staging, &target, published_major, &release);
            let _ = fsutil::remove_any(&staging);
            let _ = fs::remove_file(&archive);
            result
        })
        .await?;
        tracing::info!(major = published_major, path = %runtime.executable.display(), "installed managed Java runtime");
        Ok(runtime)
    }

    async fn find_release(&self, major: u32) -> CoreResult<(u32, AdoptiumAsset)> {
        for candidate in candidate_majors(major) {
            let url = self
                .api
                .join(&format!("assets/latest/{candidate}/hotspot"))
                .map_err(|error| CoreError::JavaUnavailable(error.to_string()))?;
            let mut url = url;
            url.query_pairs_mut()
                .append_pair("architecture", adoptium_arch())
                .append_pair("image_type", "jre")
                .append_pair("os", adoptium_os())
                .append_pair("vendor", "eclipse");
            let body = match fetch_bytes(self.transport.as_ref(), &url, 4 * 1024 * 1024).await {
                Ok(body) => body,
                Err(jifercraft_net::NetError::Status { status: 404, .. }) => continue,
                Err(error) => return Err(error.into()),
            };
            let assets: Vec<AdoptiumAsset> = serde_json::from_slice(&body).map_err(|error| CoreError::Metadata {
                what: "Adoptium release list".into(),
                reason: error.to_string(),
            })?;
            if let Some(asset) = assets.into_iter().find(|asset| {
                Path::new(&asset.binary.package.name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
            }) {
                return Ok((candidate, asset));
            }
        }
        Err(CoreError::JavaUnavailable(format!(
            "no Temurin JRE {major} or newer is published for this platform"
        )))
    }

    fn runtime_dir(&self, major: u32) -> PathBuf {
        self.runtimes_dir.join(format!("temurin-{major}"))
    }
}

fn read_runtime(dir: &Path) -> Option<JavaRuntime> {
    let installed = store::read_json::<InstalledRuntime>(&dir.join(MANIFEST))
        .ok()
        .flatten()?;
    let executable = dir.join(&installed.executable);
    executable.is_file().then_some(JavaRuntime {
        executable,
        major: Some(installed.major),
        managed: true,
    })
}

fn candidate_majors(major: u32) -> Vec<u32> {
    let mut majors = vec![major];
    majors.extend(PUBLISHED_MAJORS.iter().copied().filter(|candidate| *candidate > major));
    majors.dedup();
    majors
}

/// An explicit path from the instance settings. `java.exe` is swapped for its `javaw.exe` sibling so no
/// console window pops up next to the game.
fn explicit(path: &str) -> CoreResult<JavaRuntime> {
    let mut executable = PathBuf::from(path);
    if executable
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("java.exe"))
    {
        let windowless = executable.with_file_name("javaw.exe");
        if windowless.is_file() {
            executable = windowless;
        }
    }
    if !executable.is_file() {
        return Err(CoreError::JavaUnavailable(format!(
            "{} does not exist",
            executable.display()
        )));
    }
    Ok(JavaRuntime {
        executable,
        major: None,
        managed: false,
    })
}

fn unpack(archive: &Path, staging: &Path, target: &Path, major: u32, release: &str) -> CoreResult<JavaRuntime> {
    fsutil::remove_any(staging).at(staging)?;
    fs::create_dir_all(staging).at(staging)?;
    let file = fs::File::open(archive).at(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| bad_archive(archive, &error))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| bad_archive(archive, &error))?;
        // `enclosed_name` rejects absolute paths and `..`, closing the zip-slip hole.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let out = staging.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out).at(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).at(parent)?;
        }
        let mut output = fs::File::create(&out).at(&out)?;
        io::copy(&mut entry, &mut output).at(&out)?;
    }

    let home = find_java_home(staging)?.ok_or_else(|| {
        CoreError::JavaUnavailable(format!("{} does not contain bin/{EXECUTABLE}", archive.display()))
    })?;
    fsutil::remove_any(target).at(target)?;
    fs::rename(&home, target).at(target)?;
    let executable = Path::new("bin").join(EXECUTABLE);
    store::write_json_atomic(
        &target.join(MANIFEST),
        &InstalledRuntime {
            major,
            release: release.to_owned(),
            executable: executable.to_string_lossy().into_owned(),
        },
    )?;
    Ok(JavaRuntime {
        executable: target.join(executable),
        major: Some(major),
        managed: true,
    })
}

/// Archives wrap the runtime in one top-level folder (`jdk-17.0.21+9-jre/`); accept either layout.
fn find_java_home(staging: &Path) -> CoreResult<Option<PathBuf>> {
    if staging.join("bin").join(EXECUTABLE).is_file() {
        return Ok(Some(staging.to_path_buf()));
    }
    for entry in fs::read_dir(staging).at(staging)?.flatten() {
        let path = entry.path();
        if path.join("bin").join(EXECUTABLE).is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn file_name(name: &str) -> CoreResult<&str> {
    let valid = !name.is_empty() && Path::new(name).file_name().is_some_and(|file| file == name);
    if valid {
        Ok(name)
    } else {
        Err(CoreError::Metadata {
            what: "Adoptium package name".into(),
            reason: name.to_owned(),
        })
    }
}

fn bad_archive(path: &Path, error: &zip::result::ZipError) -> CoreError {
    CoreError::JavaUnavailable(format!("{} is not a valid archive: {error}", path.display()))
}

fn adoptium_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "mac",
        _ => "linux",
    }
}

fn adoptium_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "x32",
        "aarch64" => "aarch64",
        _ => "x64",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::MemoryTransport;
    use std::io::Write;

    fn jre_zip() -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("jdk-17.0.21+9-jre/bin/javaw.exe", "w"),
            ("jdk-17.0.21+9-jre/bin/java", "j"),
            ("jdk-17.0.21+9-jre/release", "JAVA_VERSION=\"17.0.21\""),
            ("../outside.txt", "evil"),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn sha256(bytes: &[u8]) -> String {
        let mut hasher = Checksum::Sha256([0; 32]).hasher();
        hasher.update(bytes);
        hasher.finish().to_hex()
    }

    fn serve_release(transport: &MemoryTransport, major: u32, body: &[u8]) {
        let listing = format!(
            r#"[{{"binary":{{"package":{{"checksum":"{}","link":"https://dl.test/jre-{major}.zip","name":"jre-{major}.zip","size":{}}}}},"release_name":"jdk-{major}"}}]"#,
            sha256(body),
            body.len()
        );
        let os = adoptium_os();
        let arch = adoptium_arch();
        transport.serve(
            &format!("https://api.test/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse"),
            listing,
        );
        transport.serve(&format!("https://dl.test/jre-{major}.zip"), body.to_vec());
    }

    fn manager(transport: &Arc<MemoryTransport>, dir: &Path) -> RuntimeManager<MemoryTransport> {
        RuntimeManager::new(Arc::clone(transport), dir.join("runtimes"), dir.join("cache"))
            .unwrap()
            .with_api(Url::parse("https://api.test/v3/").unwrap())
            .with_retry(RetryPolicy {
                retries: 1,
                base_delay: std::time::Duration::ZERO,
            })
    }

    fn quiet() -> ProgressFn {
        Arc::new(|_| {})
    }

    #[test]
    fn falls_back_to_published_majors() {
        assert_eq!(candidate_majors(16), [16, 17, 21, 25]);
        assert_eq!(candidate_majors(21), [21, 25]);
        assert_eq!(candidate_majors(8), [8, 11, 17, 21, 25]);
    }

    #[test]
    fn explicit_paths_prefer_javaw_and_must_exist() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("java.exe"), "").unwrap();
        fs::write(bin.join("javaw.exe"), "").unwrap();
        let runtime = explicit(&bin.join("java.exe").to_string_lossy()).unwrap();
        assert_eq!(runtime.executable, bin.join("javaw.exe"));
        assert!(!runtime.managed);
        assert_eq!(
            explicit("Z:\\missing\\java.exe").unwrap_err().code(),
            "java_unavailable"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn installs_a_verified_runtime_once_and_reuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let transport = MemoryTransport::new();
        serve_release(&transport, 17, &jre_zip());
        let (os, arch) = (adoptium_os(), adoptium_arch());
        transport.serve(
            &format!(
                "https://api.test/v3/assets/latest/16/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse"
            ),
            "[]",
        );
        let manager = manager(&transport, dir.path());

        let runtime = manager
            .resolve("", 16, &CancellationToken::new(), quiet())
            .await
            .unwrap();
        let home = dir.path().join("runtimes").join("temurin-17");
        assert_eq!(
            runtime,
            JavaRuntime {
                executable: home.join("bin").join("javaw.exe"),
                major: Some(17),
                managed: true
            }
        );
        assert!(home.join("release").is_file());
        assert!(!dir.path().join("runtimes").join("outside.txt").exists());
        assert!(
            !dir.path().join("cache").join("jre-17.zip").exists(),
            "the archive is removed after unpacking"
        );

        let again = manager
            .resolve("", 17, &CancellationToken::new(), quiet())
            .await
            .unwrap();
        assert_eq!(again, runtime);
        assert_eq!(transport.hits("https://dl.test/jre-17.zip"), 1);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn an_installed_newer_runtime_does_not_replace_the_required_major() {
        let dir = tempfile::tempdir().unwrap();
        let transport = MemoryTransport::new();
        serve_release(&transport, 17, &jre_zip());
        serve_release(&transport, 8, &jre_zip());
        let manager = manager(&transport, dir.path());

        let modern = manager
            .resolve("", 17, &CancellationToken::new(), quiet())
            .await
            .unwrap();
        let legacy = manager
            .resolve("", 8, &CancellationToken::new(), quiet())
            .await
            .unwrap();

        assert_eq!(modern.major, Some(17));
        assert_eq!(legacy.major, Some(8));
        assert!(
            legacy
                .executable
                .starts_with(dir.path().join("runtimes").join("temurin-8"))
        );
        assert_eq!(transport.hits("https://dl.test/jre-8.zip"), 1);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn offline_falls_back_to_a_newer_installed_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let online = MemoryTransport::new();
        serve_release(&online, 21, &jre_zip());
        manager(&online, dir.path())
            .resolve("", 21, &CancellationToken::new(), quiet())
            .await
            .unwrap();

        let offline = manager(&MemoryTransport::new(), dir.path());
        let runtime = offline
            .resolve("", 17, &CancellationToken::new(), quiet())
            .await
            .unwrap();
        assert_eq!(runtime.major, Some(21));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn refuses_a_tampered_archive() {
        let dir = tempfile::tempdir().unwrap();
        let transport = MemoryTransport::new();
        serve_release(&transport, 21, &jre_zip());
        let mut tampered = jre_zip();
        if let Some(byte) = tampered.last_mut() {
            *byte ^= 0xff;
        }
        transport.serve("https://dl.test/jre-21.zip", tampered);

        let error = manager(&transport, dir.path())
            .resolve("", 21, &CancellationToken::new(), quiet())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "checksum_mismatch");
        assert!(!dir.path().join("runtimes").join("temurin-21").exists());
    }
}
