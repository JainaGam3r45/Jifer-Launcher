// Launch coordinator: exactly one game running at a time because running two instances
// of modded Minecraft simultaneously is a proven recipe for an impromptu jet engine impression.
//
// States: idle -> installing -> launching -> running -> idle.
// If the user clicks "Stop" while downloading 800MB of assets, we cancel gracefully
// instead of letting ghost download threads burn bandwidth out of spite.

use std::sync::{Arc, Mutex, PoisonError};

use jifercraft_net::{DownloadSnapshot, Downloader, ProgressFn, Transport};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::error::{CoreError, CoreResult, blocking};
use crate::install::{self, InstallPlan};
use crate::instance::{Instance, LoaderType};
use crate::instances::{InstanceLease, InstanceStore};
use crate::java::RuntimeManager;
use crate::launch::{LaunchAuth, LaunchSpec, build_command};
use crate::meta::MetadataClient;
use crate::paths::{InstanceId, LauncherPaths};
use crate::process::{self, GameLogLine};
use crate::settings::SettingsStore;
use crate::version::Environment;
use crate::version::VersionJson;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchState {
    Idle,
    Installing,
    Launching,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStateEvent {
    pub state: LaunchState,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    // Stable error code so the frontend shows a nice localized toast instead of raw panic text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchPhase {
    Metadata,
    Java,
    Files,
    Natives,
    Starting,
}

// Live download stats and phase info to keep impatient players from mashing the Play button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProgress {
    pub phase: LaunchPhase,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<DownloadTelemetry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTelemetry {
    pub files_total: u64,
    pub files_done: u64,
    pub files_reused: u64,
    pub files_active: u64,
    pub files_verifying: u64,
    pub files_verified: u64,
    pub retries: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_per_second: u64,
}

impl From<DownloadSnapshot> for DownloadTelemetry {
    fn from(snapshot: DownloadSnapshot) -> Self {
        Self {
            files_total: snapshot.files_total,
            files_done: snapshot.files_done,
            files_reused: snapshot.files_reused,
            files_active: snapshot.files_active,
            files_verifying: snapshot.files_verifying,
            files_verified: snapshot.files_verified,
            retries: snapshot.retries,
            bytes_total: snapshot.bytes_total,
            bytes_done: snapshot.bytes_done,
            bytes_per_second: snapshot.bytes_per_second,
        }
    }
}

// Observer trait: whenever Minecraft sneezes, logs a line, or downloads an asset, notify the UI.
pub trait LaunchObserver: Send + Sync + 'static {
    fn state(&self, event: &LaunchStateEvent);
    fn progress(&self, event: &LaunchProgress);
    fn log(&self, line: GameLogLine);
}

#[derive(Debug, Clone)]
pub struct LauncherIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug)]
struct Active {
    instance_id: InstanceId,
    cancel: CancellationToken,
}

pub struct GameLauncher<T> {
    paths: LauncherPaths,
    transport: Arc<T>,
    metadata: Arc<MetadataClient<T>>,
    runtimes: Arc<RuntimeManager<T>>,
    instances: Arc<InstanceStore>,
    settings: Arc<SettingsStore>,
    identity: LauncherIdentity,
    active: Mutex<Option<Active>>,
    last_state: Mutex<LaunchStateEvent>,
}

impl<T> std::fmt::Debug for GameLauncher<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameLauncher")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl<T: Transport> GameLauncher<T> {
    pub fn new(
        paths: LauncherPaths,
        transport: Arc<T>,
        metadata: Arc<MetadataClient<T>>,
        runtimes: Arc<RuntimeManager<T>>,
        instances: Arc<InstanceStore>,
        settings: Arc<SettingsStore>,
        identity: LauncherIdentity,
    ) -> Self {
        Self {
            paths,
            transport,
            metadata,
            runtimes,
            instances,
            settings,
            identity,
            active: Mutex::new(None),
            last_state: Mutex::new(event(LaunchState::Idle, "Ready.", None)),
        }
    }

    pub fn state(&self) -> LaunchStateEvent {
        self.last_state.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The instance currently being prepared or played, if any.
    pub fn active_instance(&self) -> Option<InstanceId> {
        self.active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|active| active.instance_id.clone())
    }

    /// Prepares and starts the game, returning once it is running. The process is then supervised in the
    /// background until it exits.
    pub async fn launch(
        self: &Arc<Self>,
        id: &InstanceId,
        auth: LaunchAuth,
        observer: Arc<dyn LaunchObserver>,
    ) -> CoreResult<()> {
        let cancel = self.claim(id)?;
        match self.prepare_and_spawn(id, auth, &cancel, &observer).await {
            Ok(running) => {
                let launcher = Arc::clone(self);
                tokio::spawn(async move { launcher.supervise(running, observer).await });
                Ok(())
            }
            Err(error) => {
                self.release();
                if matches!(error, CoreError::Cancelled) {
                    self.emit(&observer, event(LaunchState::Idle, "Launch cancelled.", Some(id)));
                } else {
                    let mut failed = event(LaunchState::Error, &error.to_string(), Some(id));
                    failed.code = Some(error.code().to_owned());
                    self.emit(&observer, failed);
                }
                Err(error)
            }
        }
    }

    /// Cancels preparation or kills the running game. Returns whether anything was active.
    pub fn stop(&self, observer: &Arc<dyn LaunchObserver>) -> bool {
        let active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(active) = active.as_ref() else { return false };
        self.emit(
            observer,
            event(LaunchState::Stopping, "Stopping Minecraft.", Some(&active.instance_id)),
        );
        active.cancel.cancel();
        true
    }

    fn claim(&self, id: &InstanceId) -> CoreResult<CancellationToken> {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if active.is_some() {
            return Err(CoreError::AlreadyRunning);
        }
        let cancel = CancellationToken::new();
        *active = Some(Active {
            instance_id: id.clone(),
            cancel: cancel.clone(),
        });
        Ok(cancel)
    }

    fn release(&self) {
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn emit(&self, observer: &Arc<dyn LaunchObserver>, next: LaunchStateEvent) {
        observer.state(&next);
        *self.last_state.lock().unwrap_or_else(PoisonError::into_inner) = next;
    }

    async fn prepare_and_spawn(
        &self,
        id: &InstanceId,
        auth: LaunchAuth,
        cancel: &CancellationToken,
        observer: &Arc<dyn LaunchObserver>,
    ) -> CoreResult<Running> {
        let (instances, lookup) = (Arc::clone(&self.instances), id.clone());
        let instance = blocking(move || instances.get(&lookup)).await?;
        if instance.fields.loader_type != LoaderType::Vanilla {
            return Err(CoreError::LoaderUnsupported(
                format!("{:?}", instance.fields.loader_type).to_lowercase(),
            ));
        }
        let version_id = instance.fields.version_id.clone();
        if version_id.is_empty() {
            return Err(CoreError::MissingVersion);
        }
        let (instances, lookup) = (Arc::clone(&self.instances), id.clone());
        let lease = blocking(move || instances.lease(&lookup)).await?;

        self.emit(
            observer,
            event(
                LaunchState::Installing,
                &format!("Preparing Minecraft {version_id}."),
                Some(id),
            ),
        );
        let settings = Arc::clone(&self.settings);
        let root = blocking(move || settings.get()).await?.minecraft_root();
        let env = Environment::current();
        let (version, plan) = self.prepare_files(&instance, &root, &env, cancel, observer).await?;

        report(
            observer,
            LaunchPhase::Java,
            &format!("Preparing Java {}.", version.java_major()),
            None,
        );
        let java = self
            .runtimes
            .resolve(
                &instance.fields.java_path,
                version.java_major(),
                cancel,
                files_progress(observer, LaunchPhase::Java, "Downloading Java."),
            )
            .await
            .map_err(|error| {
                if cancel.is_cancelled() {
                    CoreError::Cancelled
                } else {
                    error
                }
            })?;
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        self.emit(
            observer,
            event(
                LaunchState::Launching,
                &format!("Launching Minecraft {version_id}."),
                Some(id),
            ),
        );
        report(observer, LaunchPhase::Starting, "Starting Minecraft.", None);
        let window = instance
            .fields
            .window
            .or_else(|| self.settings.get().ok().map(|settings| settings.window));
        let command = build_command(&LaunchSpec {
            version: &version,
            plan: &plan,
            java: &java.executable,
            minecraft_root: &root,
            game_dir: &instance.game_dir,
            auth: &auth,
            memory: &instance.fields.memory_config,
            window,
            env,
            launcher_name: &self.identity.name,
            launcher_version: &self.identity.version,
        });
        tracing::info!(java = %java.executable.display(), args = ?command.redacted_args(), "starting Minecraft");
        let sink_observer = Arc::clone(observer);
        let log_dir = self.paths.logs_dir().join(id.as_str());
        let game = process::spawn(
            &command,
            &log_dir,
            Arc::new(move |line| sink_observer.log(line)),
            cancel.clone(),
        )?;

        let (instances, lookup) = (Arc::clone(&self.instances), id.clone());
        if let Err(error) = blocking(move || instances.mark_played(&lookup)).await {
            tracing::warn!(%error, "could not record the last played time");
        }
        self.emit(
            observer,
            event(
                LaunchState::Running,
                &format!("Minecraft {version_id} is running."),
                Some(id),
            ),
        );
        Ok(Running { instance, game, lease })
    }

    /// Metadata, verified downloads, natives and legacy assets for the instance's version.
    async fn prepare_files(
        &self,
        instance: &Instance,
        root: &std::path::Path,
        env: &Environment,
        cancel: &CancellationToken,
        observer: &Arc<dyn LaunchObserver>,
    ) -> CoreResult<(VersionJson, InstallPlan)> {
        report(observer, LaunchPhase::Metadata, "Reading version metadata.", None);
        let version = cancellable(cancel, self.metadata.version(root, &instance.fields.version_id)).await?;
        let plan = install::plan_version(&version, root, env)?;
        let mut jobs = plan.jobs.clone();
        let asset_index = match &version.asset_index {
            Some(reference) => {
                let index = cancellable(cancel, self.metadata.asset_index(root, reference)).await?;
                jobs.extend(install::plan_assets(&index, root)?);
                Some(index)
            }
            None => None,
        };

        report(observer, LaunchPhase::Files, "Verifying game files.", None);
        Downloader::new(Arc::clone(&self.transport))
            .run(
                jobs,
                cancel,
                files_progress(observer, LaunchPhase::Files, "Downloading game files."),
            )
            .await
            .map_err(|error| {
                if cancel.is_cancelled() {
                    CoreError::Cancelled
                } else {
                    error.into()
                }
            })?;

        report(observer, LaunchPhase::Natives, "Unpacking native libraries.", None);
        let (archives, natives_dir) = (plan.native_archives.clone(), plan.natives_dir.clone());
        blocking(move || install::extract_natives(&archives, &natives_dir)).await?;
        if let Some(index) = asset_index {
            let (root, game_dir, assets_id) = (
                root.to_path_buf(),
                instance.game_dir.clone(),
                version.assets_id().to_owned(),
            );
            blocking(move || install::materialize_legacy_assets(&index, &root, &assets_id, &game_dir)).await?;
        }
        Ok((version, plan))
    }

    async fn supervise(self: Arc<Self>, running: Running, observer: Arc<dyn LaunchObserver>) {
        let id = running.instance.id.clone();
        let message = match running.game.wait().await {
            Ok(exit) if exit.killed => "Minecraft was stopped.".to_owned(),
            Ok(exit) => match exit.code {
                Some(code) => format!("Minecraft closed with code {code}."),
                None => "Minecraft closed.".to_owned(),
            },
            Err(error) => format!("Minecraft closed unexpectedly: {error}"),
        };
        drop(running.lease);
        self.release();
        self.emit(&observer, event(LaunchState::Idle, &message, Some(&id)));
    }
}

struct Running {
    instance: Instance,
    game: process::RunningGame,
    lease: InstanceLease,
}

fn event(state: LaunchState, message: &str, instance: Option<&InstanceId>) -> LaunchStateEvent {
    LaunchStateEvent {
        state,
        message: message.to_owned(),
        instance_id: instance.map(ToString::to_string),
        code: None,
    }
}

fn report(observer: &Arc<dyn LaunchObserver>, phase: LaunchPhase, message: &str, download: Option<DownloadTelemetry>) {
    observer.progress(&LaunchProgress {
        phase,
        message: message.to_owned(),
        download,
    });
}

fn files_progress(observer: &Arc<dyn LaunchObserver>, phase: LaunchPhase, message: &'static str) -> ProgressFn {
    let observer = Arc::clone(observer);
    Arc::new(move |snapshot| report(&observer, phase, message, Some(snapshot.into())))
}

async fn cancellable<V>(cancel: &CancellationToken, work: impl Future<Output = CoreResult<V>>) -> CoreResult<V> {
    tokio::select! {
        result = work => result,
        () = cancel.cancelled() => Err(CoreError::Cancelled),
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;
    use crate::instance::CreateInstanceInput;
    use crate::testkit::{MemoryTransport, sha1_hex};
    use jifercraft_net::Url;
    use std::time::Duration;

    #[derive(Default)]
    struct Recorder {
        states: Mutex<Vec<LaunchStateEvent>>,
        progress: Mutex<Vec<LaunchProgress>>,
        logs: Mutex<Vec<GameLogLine>>,
    }

    impl LaunchObserver for Recorder {
        fn state(&self, event: &LaunchStateEvent) {
            self.states.lock().unwrap().push(event.clone());
        }
        fn progress(&self, event: &LaunchProgress) {
            self.progress.lock().unwrap().push(event.clone());
        }
        fn log(&self, line: GameLogLine) {
            self.logs.lock().unwrap().push(line);
        }
    }

    impl Recorder {
        fn states(&self) -> Vec<LaunchState> {
            self.states.lock().unwrap().iter().map(|event| event.state).collect()
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        launcher: Arc<GameLauncher<MemoryTransport>>,
        instances: Arc<InstanceStore>,
    }

    fn serve(transport: &MemoryTransport, url: &str, body: &[u8]) -> (String, usize) {
        transport.serve(url, body.to_vec());
        (sha1_hex(body), body.len())
    }

    /// A fake Mojang plus a "Java" that is really `cmd.exe`: given JVM flags and no stdin it prints its banner
    /// and exits, which exercises the whole pipeline without a JDK.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::at(dir.path());
        let transport = MemoryTransport::new();
        let asset = b"sound".to_vec();
        let asset_hash = sha1_hex(&asset);
        transport.serve(
            &format!("{}/{}/{asset_hash}", install::ASSET_HOST, &asset_hash[..2]),
            asset.clone(),
        );
        let index = format!(
            "{{\"objects\":{{\"minecraft/sounds/a.ogg\":{{\"hash\":\"{asset_hash}\",\"size\":{}}}}}}}",
            asset.len()
        );
        let (client_sha, client_len) = serve(&transport, "https://cdn.test/client.jar", b"client-jar");
        let (lib_sha, lib_len) = serve(&transport, "https://cdn.test/lib.jar", b"library");
        let (log_sha, log_len) = serve(&transport, "https://cdn.test/log.xml", b"<xml/>");
        let (index_sha, index_len) = serve(&transport, "https://cdn.test/index.json", index.as_bytes());
        let version = serde_json::json!({
            "id": "1.20.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name}",
            "downloads": { "client": { "sha1": client_sha, "size": client_len, "url": "https://cdn.test/client.jar" } },
            "assetIndex": { "id": "5", "sha1": index_sha, "size": index_len, "url": "https://cdn.test/index.json" },
            "logging": { "client": { "argument": "-Dlog4j.configurationFile=${path}",
                "file": { "id": "client.xml", "sha1": log_sha, "size": log_len, "url": "https://cdn.test/log.xml" } } },
            "libraries": [{ "name": "com.mojang:lib:1.0", "downloads": { "artifact": {
                "path": "com/mojang/lib/1.0/lib-1.0.jar", "sha1": lib_sha, "size": lib_len, "url": "https://cdn.test/lib.jar" } } }]
        })
        .to_string();
        let (version_sha, _) = serve(&transport, "https://cdn.test/1.20.1.json", version.as_bytes());
        let manifest = serde_json::json!({
            "latest": { "release": "1.20.1", "snapshot": "1.20.1" },
            "versions": [{ "id": "1.20.1", "type": "release", "url": "https://cdn.test/1.20.1.json",
                "time": "t", "releaseTime": "t", "sha1": version_sha }]
        })
        .to_string();
        transport.serve("https://cdn.test/manifest.json", manifest);

        let metadata = Arc::new(
            MetadataClient::new(Arc::clone(&transport), &paths.cache_dir())
                .unwrap()
                .with_manifest_url(Url::parse("https://cdn.test/manifest.json").unwrap()),
        );
        let runtimes =
            Arc::new(RuntimeManager::new(Arc::clone(&transport), paths.runtimes_dir(), paths.cache_dir()).unwrap());
        let instances = Arc::new(InstanceStore::new(paths.instances_dir()));
        let settings = Arc::new(SettingsStore::new(paths.clone()));
        let root = settings.get().unwrap().minecraft_root();
        let identity = LauncherIdentity {
            name: "jifercraft-launcher".into(),
            version: "0.0.0-test".into(),
        };
        let launcher = Arc::new(GameLauncher::new(
            paths,
            transport,
            metadata,
            runtimes,
            Arc::clone(&instances),
            settings,
            identity,
        ));
        Fixture {
            _dir: dir,
            root,
            launcher,
            instances,
        }
    }

    fn instance(store: &InstanceStore, loader: &str) -> Instance {
        let cmd = std::path::Path::new(&std::env::var("SystemRoot").unwrap())
            .join("System32")
            .join("cmd.exe");
        let mut input =
            serde_json::json!({ "name": "Test", "versionId": "1.20.1", "loaderType": loader, "javaPath": cmd });
        if loader != "vanilla" {
            input["loaderVersion"] = "0.15.0".into();
        }
        store
            .create(serde_json::from_value::<CreateInstanceInput>(input).unwrap())
            .unwrap()
    }

    fn auth() -> LaunchAuth {
        LaunchAuth {
            username: "Steve".into(),
            uuid: "00000000-0000-0000-0000-000000000000".into(),
            access_token: "token".into(),
            user_type: "legacy".into(),
            xuid: None,
            client_id: None,
        }
    }

    async fn wait_for_idle(recorder: &Recorder) {
        for _ in 0..200 {
            if recorder.states().last() == Some(&LaunchState::Idle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the game never returned to idle: {:?}", recorder.states());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn installs_verifies_launches_and_returns_to_idle() {
        let fixture = fixture();
        let created = instance(&fixture.instances, "vanilla");
        let recorder = Arc::new(Recorder::default());
        let observer: Arc<dyn LaunchObserver> = recorder.clone();

        fixture
            .launcher
            .launch(&created.id, auth(), Arc::clone(&observer))
            .await
            .unwrap();
        wait_for_idle(&recorder).await;

        assert_eq!(
            recorder.states(),
            [
                LaunchState::Installing,
                LaunchState::Launching,
                LaunchState::Running,
                LaunchState::Idle
            ]
        );
        assert!(
            fixture
                .root
                .join("versions")
                .join("1.20.1")
                .join("1.20.1.jar")
                .is_file()
        );
        assert!(
            fixture
                .root
                .join("libraries")
                .join("com/mojang/lib/1.0/lib-1.0.jar")
                .is_file()
        );
        assert!(
            fixture
                .root
                .join("assets")
                .join("log_configs")
                .join("client.xml")
                .is_file()
        );
        let files = recorder
            .progress
            .lock()
            .unwrap()
            .iter()
            .filter_map(|p| p.download)
            .next_back()
            .unwrap();
        assert_eq!((files.files_total, files.files_done, files.files_verified), (4, 4, 4));
        assert!(
            !recorder.logs.lock().unwrap().is_empty(),
            "process output reaches the observer"
        );
        assert!(fixture.instances.get(&created.id).unwrap().last_played_at.is_some());
        assert_eq!(fixture.launcher.active_instance(), None);
        assert!(
            fixture.instances.lease(&created.id).is_ok(),
            "the lease is released after exit"
        );
    }

    #[tokio::test]
    async fn modded_instances_are_refused_with_a_code() {
        let fixture = fixture();
        let created = instance(&fixture.instances, "fabric");
        let recorder = Arc::new(Recorder::default());

        let error = fixture
            .launcher
            .launch(&created.id, auth(), recorder.clone())
            .await
            .unwrap_err();

        assert_eq!(error.code(), "loader_unsupported");
        let last = recorder.states.lock().unwrap().last().cloned().unwrap();
        assert_eq!(
            (last.state, last.code.as_deref()),
            (LaunchState::Error, Some("loader_unsupported"))
        );
        assert_eq!(fixture.launcher.active_instance(), None);
    }

    #[tokio::test]
    async fn only_one_game_at_a_time() {
        let fixture = fixture();
        let created = instance(&fixture.instances, "vanilla");
        let _claim = fixture.launcher.claim(&created.id).unwrap();
        let error = fixture
            .launcher
            .launch(&created.id, auth(), Arc::new(Recorder::default()))
            .await
            .unwrap_err();
        assert_eq!(error.code(), "already_running");
    }

    #[tokio::test]
    async fn stop_cancels_whatever_is_active() {
        let fixture = fixture();
        let created = instance(&fixture.instances, "vanilla");
        let recorder = Arc::new(Recorder::default());
        let observer: Arc<dyn LaunchObserver> = recorder.clone();
        let cancel = fixture.launcher.claim(&created.id).unwrap();
        assert!(fixture.launcher.stop(&observer));
        assert!(cancel.is_cancelled());
        assert_eq!(recorder.states(), [LaunchState::Stopping]);
        fixture.launcher.release();
        assert!(!fixture.launcher.stop(&observer));
    }
}
