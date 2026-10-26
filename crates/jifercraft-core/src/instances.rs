// Instance management on disk, atomic manifests, and preventing players from
// accidentally deleting or modifying an instance while Minecraft is actively running.
//
// We use a real OS file lock (`instances/<id>/.lock`) while the game is alive.
// If someone launches two copies or tries to delete a world mid-game, Windows Defender
// won't save them, but our lock file will.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::error::{CoreError, CoreResult, IoContext};
use crate::fsutil;
use crate::instance::{
    self, CloneInstanceInput, CreateInstanceInput, DEFAULT_ICON, Instance, InstanceFields, LoaderType,
    UpdateInstanceInput,
};
use crate::paths::{InstanceId, assert_inside};
use crate::settings::LauncherSettings;
use crate::store;
use crate::timestamp;
use crate::validate;

const MANIFEST: &str = "instance.json";
const GAME_DIR: &str = "minecraft";
const LOCK_FILE: &str = ".lock";
pub const DEFAULT_INSTANCE_ID: &str = "default";
const CLONE_LIGHT_ENTRIES: [&str; 3] = ["mods", "config", "options.txt"];
const LEGACY_ENTRIES: [&str; 7] = [
    "saves",
    "mods",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "options.txt",
    "servers.dat",
];

#[derive(Debug)]
pub struct InstanceStore {
    root: PathBuf,
    writers: Mutex<HashMap<InstanceId, Arc<Mutex<()>>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefaultInstance {
    /// Whether `default` was created on this call.
    pub migrated: bool,
    /// How many legacy entries (worlds, mods, options…) were moved into it.
    pub moved_entries: usize,
    pub instance: Instance,
}

/// Exclusive OS-level claim on an instance; released on drop.
#[derive(Debug)]
pub struct InstanceLease {
    id: InstanceId,
    _file: File,
}

impl InstanceLease {
    pub fn id(&self) -> &InstanceId {
        &self.id
    }
}

impl InstanceStore {
    pub fn new(instances_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: instances_dir.into(),
            writers: Mutex::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn instance_dir(&self, id: &InstanceId) -> CoreResult<PathBuf> {
        assert_inside(&self.root, &self.root.join(id.as_str()))
    }

    pub fn game_dir(&self, id: &InstanceId) -> CoreResult<PathBuf> {
        Ok(self.instance_dir(id)?.join(GAME_DIR))
    }

    /// All readable instances, oldest first. Unreadable manifests are skipped with a warning.
    pub fn list(&self) -> CoreResult<Vec<Instance>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(CoreError::io(&self.root, error)),
        };
        let mut instances = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|name| InstanceId::parse(name).ok()) else {
                continue;
            };
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            match self.get(&id) {
                Ok(instance) => instances.push(instance),
                Err(error) => tracing::warn!(instance = %id, %error, "skipping unreadable instance"),
            }
        }
        instances.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.fields.name.cmp(&b.fields.name))
        });
        Ok(instances)
    }

    pub fn get(&self, id: &InstanceId) -> CoreResult<Instance> {
        let dir = self.instance_dir(id)?;
        let manifest = dir.join(MANIFEST);
        store::discard_stale_temp(&manifest);
        let bytes = match fs::read(&manifest) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(CoreError::InstanceNotFound(id.to_string()));
            }
            Err(error) => return Err(CoreError::io(manifest, error)),
        };
        instance::parse_manifest(id, dir.join(GAME_DIR), &bytes)
    }

    pub fn create(&self, input: CreateInstanceInput) -> CoreResult<Instance> {
        let fields = input.into_fields()?;
        let id = new_id()?;
        let writer = self.writer(&id);
        let _guard = lock(&writer);
        let instance = self.build(id, fields)?;
        self.create_in_directory(&instance, |_| Ok(()))?;
        Ok(instance)
    }

    pub fn clone_instance(&self, input: &CloneInstanceInput) -> CoreResult<Instance> {
        let source_id = InstanceId::parse(&input.source_id)?;
        let name = instance::validate_name(&input.name)?;
        let source = self.get(&source_id)?;
        // A running game rewrites worlds and configs; copying underneath it would snapshot torn files.
        let _source_lease = self.lease(&source_id)?;
        let id = new_id()?;
        let writer = self.writer(&id);
        let _guard = lock(&writer);
        let instance = self.build(
            id,
            InstanceFields {
                name,
                ..source.fields.clone()
            },
        )?;
        self.create_in_directory(&instance, |game_dir| {
            if input.copy_worlds {
                fsutil::copy_if_exists(&source.game_dir, game_dir).at(&source.game_dir)
            } else {
                CLONE_LIGHT_ENTRIES.iter().try_for_each(|entry| {
                    fsutil::copy_if_exists(&source.game_dir.join(entry), &game_dir.join(entry)).at(game_dir.join(entry))
                })
            }
        })?;
        Ok(instance)
    }

    pub fn update(&self, id: &InstanceId, patch: UpdateInstanceInput) -> CoreResult<Instance> {
        self.modify(id, |current| {
            Ok(Instance {
                fields: patch.apply_to(&current.fields)?,
                ..current.clone()
            })
        })
    }

    pub fn mark_played(&self, id: &InstanceId) -> CoreResult<Instance> {
        self.modify(id, |current| {
            Ok(Instance {
                last_played_at: Some(timestamp::now_iso()),
                ..current.clone()
            })
        })
    }

    /// Deletes the instance folder. Refused while a game or another process holds the lease.
    pub fn remove(&self, id: &InstanceId) -> CoreResult<()> {
        let dir = self.instance_dir(id)?;
        let writer = self.writer(id);
        let _guard = lock(&writer);
        if !fsutil::exists(&dir).at(&dir)? {
            return Err(CoreError::InstanceNotFound(id.to_string()));
        }
        let lease = self.lease(id)?;
        // Windows cannot delete a file with an open handle, so everything but the lock goes first.
        for entry in fs::read_dir(&dir).at(&dir)?.flatten() {
            if entry.file_name() != LOCK_FILE {
                fsutil::remove_any(&entry.path()).at(entry.path())?;
            }
        }
        drop(lease);
        fs::remove_dir_all(&dir).at(&dir)
    }

    /// Claims the instance for a running game (or another long exclusive operation).
    pub fn lease(&self, id: &InstanceId) -> CoreResult<InstanceLease> {
        let dir = self.instance_dir(id)?;
        let path = dir.join(LOCK_FILE);
        fs::create_dir_all(&dir).at(&dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .at(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(InstanceLease {
                id: id.clone(),
                _file: file,
            }),
            Err(TryLockError::WouldBlock) => Err(CoreError::InstanceBusy(id.to_string())),
            Err(TryLockError::Error(error)) => Err(CoreError::io(path, error)),
        }
    }

    /// Picks the instance to activate on startup, creating `default` from legacy settings when there is none.
    pub fn ensure_default(&self, settings: &LauncherSettings) -> CoreResult<DefaultInstance> {
        if let Some(active) = self.find_usable(&settings.active_instance_id)? {
            return Ok(DefaultInstance {
                migrated: false,
                moved_entries: 0,
                instance: active,
            });
        }
        if let Some(latest) = latest(self.list()?) {
            return Ok(DefaultInstance {
                migrated: false,
                moved_entries: 0,
                instance: latest,
            });
        }
        let id = InstanceId::parse(DEFAULT_INSTANCE_ID)?;
        let writer = self.writer(&id);
        let _guard = lock(&writer);
        let instance = self.build(id, legacy_fields(settings)?)?;
        fs::create_dir_all(&instance.game_dir).at(&instance.game_dir)?;
        let moved_entries = move_legacy_data(&settings.minecraft_root(), &instance.game_dir)?;
        // The manifest goes last: an interrupted move leaves no manifest, so the next start resumes it.
        self.write_manifest(&instance)?;
        tracing::info!("created the Default instance from the existing launcher settings");
        Ok(DefaultInstance {
            migrated: true,
            moved_entries,
            instance,
        })
    }

    fn modify(&self, id: &InstanceId, change: impl FnOnce(&Instance) -> CoreResult<Instance>) -> CoreResult<Instance> {
        let writer = self.writer(id);
        let _guard = lock(&writer);
        let current = self.get(id)?;
        let next = change(&current)?;
        self.write_manifest(&next)?;
        Ok(next)
    }

    fn find_usable(&self, raw_id: &str) -> CoreResult<Option<Instance>> {
        let Ok(id) = InstanceId::parse(raw_id) else {
            return Ok(None);
        };
        match self.get(&id) {
            Ok(instance) => Ok(Some(instance)),
            Err(CoreError::InstanceNotFound(_) | CoreError::InstanceCorrupt { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn build(&self, id: InstanceId, fields: InstanceFields) -> CoreResult<Instance> {
        Ok(Instance {
            schema_version: instance::SCHEMA_VERSION,
            game_dir: self.game_dir(&id)?,
            id,
            fields,
            created_at: timestamp::now_iso(),
            last_played_at: None,
        })
    }

    fn create_in_directory(
        &self,
        instance: &Instance,
        populate: impl FnOnce(&Path) -> CoreResult<()>,
    ) -> CoreResult<()> {
        let result = fs::create_dir_all(&instance.game_dir)
            .at(&instance.game_dir)
            .and_then(|()| populate(&instance.game_dir))
            .and_then(|()| self.write_manifest(instance));
        if result.is_err()
            && let Ok(dir) = self.instance_dir(&instance.id)
        {
            let _ = fsutil::remove_any(&dir);
        }
        result
    }

    fn write_manifest(&self, instance: &Instance) -> CoreResult<()> {
        store::write_json_atomic(&self.instance_dir(&instance.id)?.join(MANIFEST), instance)
    }

    fn writer(&self, id: &InstanceId) -> Arc<Mutex<()>> {
        let mut writers = self.writers.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(writers.entry(id.clone()).or_default())
    }
}

fn lock(writer: &Mutex<()>) -> MutexGuard<'_, ()> {
    // The mutex guards no data, only ordering, so a poisoned one is still safe to reuse.
    writer.lock().unwrap_or_else(PoisonError::into_inner)
}

fn new_id() -> CoreResult<InstanceId> {
    InstanceId::parse(&uuid::Uuid::new_v4().hyphenated().to_string())
}

fn latest(instances: Vec<Instance>) -> Option<Instance> {
    instances.into_iter().reduce(|best, next| {
        if next.last_activity() > best.last_activity() {
            next
        } else {
            best
        }
    })
}

fn legacy_fields(settings: &LauncherSettings) -> CoreResult<InstanceFields> {
    let version_id = if validate::is_token(&settings.selected_version, 0, 64) {
        settings.selected_version.clone()
    } else {
        String::new()
    };
    let java_path = if validate::is_safe_path_text(&settings.java_path) {
        settings.java_path.clone()
    } else {
        String::new()
    };
    let legacy = validate::MemoryConfig::new(
        &settings.memory.min.trim().to_uppercase(),
        &settings.memory.max.trim().to_uppercase(),
    );
    let memory_config = legacy.validated().unwrap_or_else(|_| {
        tracing::warn!(min = %settings.memory.min, max = %settings.memory.max, "ignoring legacy memory settings");
        instance::default_memory()
    });
    InstanceFields {
        name: "Default".to_owned(),
        version_id,
        loader_type: LoaderType::Vanilla,
        loader_version: None,
        java_path,
        memory_config,
        window: None,
        icon: DEFAULT_ICON.to_owned(),
    }
    .validated()
}

fn move_legacy_data(minecraft_root: &Path, game_dir: &Path) -> CoreResult<usize> {
    let mut moved = 0;
    if minecraft_root.as_os_str().is_empty() {
        return Ok(moved);
    }
    for entry in LEGACY_ENTRIES {
        let source = minecraft_root.join(entry);
        let target = game_dir.join(entry);
        if !fsutil::exists(&source).at(&source)? {
            continue;
        }
        if fsutil::exists(&target).at(&target)? {
            tracing::warn!(
                entry,
                "leaving legacy entry in place: the Default instance already has one"
            );
            continue;
        }
        fsutil::move_path(&source, &target).map_err(|source| CoreError::MigrationFailed {
            entry: entry.to_owned(),
            source,
        })?;
        moved += 1;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{MemoryConfig, WindowConfig};
    use std::sync::Barrier;
    use std::thread;

    fn store() -> (tempfile::TempDir, InstanceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(dir.path().join("instances"));
        (dir, store)
    }

    fn input(name: &str) -> CreateInstanceInput {
        serde_json::from_value(serde_json::json!({ "name": name, "versionId": "1.20.1", "loaderType": "vanilla" }))
            .unwrap()
    }

    fn patch(json: serde_json::Value) -> UpdateInstanceInput {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn create_persists_the_manifest_and_an_isolated_game_dir() {
        let (_dir, store) = store();
        let created = store.create(input("Survival")).unwrap();
        assert!(created.game_dir.is_dir());
        assert_eq!(
            created.game_dir,
            store.root().join(created.id.as_str()).join("minecraft")
        );
        assert_eq!(created.fields.memory_config, MemoryConfig::new("2G", "4G"));
        assert_eq!(created.fields.icon, "grass");
        assert_eq!(store.get(&created.id).unwrap(), created);
    }

    #[test]
    fn list_is_oldest_first_and_skips_corrupt_manifests() {
        let (_dir, store) = store();
        assert_eq!(store.list().unwrap().len(), 0);
        let first = store.create(input("First")).unwrap();
        let second = store.create(input("Second")).unwrap();
        let broken = store.create(input("Broken")).unwrap();
        fs::write(store.instance_dir(&broken.id).unwrap().join(MANIFEST), "{").unwrap();
        fs::create_dir_all(store.root().join("Not-Valid")).unwrap();
        let ids: Vec<_> = store.list().unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&first.id) && ids.contains(&second.id));
    }

    #[test]
    fn get_reports_missing_instances() {
        let (_dir, store) = store();
        let error = store.get(&InstanceId::parse("ghost").unwrap()).unwrap_err();
        assert_eq!(error.code(), "instance_not_found");
    }

    #[test]
    fn update_merges_and_keeps_immutable_fields() {
        let (_dir, store) = store();
        let created = store.create(input("Before")).unwrap();
        let updated = store
            .update(
                &created.id,
                patch(serde_json::json!({ "name": "After", "versionId": "1.21" })),
            )
            .unwrap();
        assert_eq!(updated.fields.name, "After");
        assert_eq!(updated.fields.version_id, "1.21");
        assert_eq!(updated.created_at, created.created_at);
        assert_eq!(updated.game_dir, created.game_dir);
    }

    #[test]
    fn invalid_updates_leave_the_manifest_untouched() {
        let (_dir, store) = store();
        let created = store.create(input("Keep")).unwrap();
        let error = store.update(
            &created.id,
            patch(serde_json::json!({ "memoryConfig": { "min": "9G", "max": "1G" } })),
        );
        assert_eq!(error.unwrap_err().code(), "invalid_instance");
        assert_eq!(store.get(&created.id).unwrap(), created);
    }

    #[test]
    fn window_override_is_kept_cleared_and_validated() {
        let (_dir, store) = store();
        let created = store.create(input("Windowed")).unwrap();
        let window = WindowConfig {
            width: 1600,
            height: 900,
            fullscreen: false,
        };
        let set = store
            .update(&created.id, patch(serde_json::json!({ "window": window })))
            .unwrap();
        assert_eq!(set.fields.window, Some(window));
        let kept = store
            .update(&created.id, patch(serde_json::json!({ "name": "Renamed" })))
            .unwrap();
        assert_eq!(kept.fields.window, Some(window));
        let cleared = store
            .update(&created.id, patch(serde_json::json!({ "window": null })))
            .unwrap();
        assert_eq!(cleared.fields.window, None);
    }

    #[test]
    fn concurrent_updates_are_serialized() {
        let (_dir, store) = store();
        let store = Arc::new(store);
        let created = store.create(input("Busy")).unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let (store, barrier, id) = (Arc::clone(&store), Arc::clone(&barrier), created.id.clone());
                thread::spawn(move || {
                    barrier.wait();
                    for round in 0..5 {
                        let name = format!("t{n}-r{round}");
                        store.update(&id, patch(serde_json::json!({ "name": name }))).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let leftovers: Vec<_> = fs::read_dir(store.instance_dir(&created.id).unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| Path::new(name).extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert_eq!(leftovers.len(), 0);
        assert!(store.get(&created.id).unwrap().fields.name.ends_with("-r4"));
    }

    #[test]
    fn clone_copies_light_entries_unless_worlds_are_requested() {
        let (_dir, store) = store();
        let source = store.create(input("Source")).unwrap();
        for entry in ["mods", "config", "saves"] {
            fs::create_dir_all(source.game_dir.join(entry)).unwrap();
            fs::write(source.game_dir.join(entry).join("file"), entry).unwrap();
        }
        fs::write(source.game_dir.join("options.txt"), "fov:90").unwrap();

        let light = store
            .clone_instance(&CloneInstanceInput {
                source_id: source.id.to_string(),
                name: "Light".into(),
                copy_worlds: false,
            })
            .unwrap();
        assert!(light.game_dir.join("mods/file").exists());
        assert!(light.game_dir.join("options.txt").exists());
        assert!(!light.game_dir.join("saves").exists());

        let full = store
            .clone_instance(&CloneInstanceInput {
                source_id: source.id.to_string(),
                name: "Full".into(),
                copy_worlds: true,
            })
            .unwrap();
        assert!(full.game_dir.join("saves/file").exists());
        assert!(source.game_dir.join("saves/file").exists());
        assert_eq!(full.fields.version_id, source.fields.version_id);
    }

    #[test]
    fn clone_of_a_missing_source_leaves_nothing_behind() {
        let (_dir, store) = store();
        let error = store
            .clone_instance(&CloneInstanceInput {
                source_id: "ghost".into(),
                name: "Copy".into(),
                copy_worlds: false,
            })
            .unwrap_err();
        assert_eq!(error.code(), "instance_not_found");
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn remove_deletes_only_that_instance_and_respects_the_lease() {
        let (_dir, store) = store();
        let keep = store.create(input("Keep")).unwrap();
        let drop_me = store.create(input("Drop")).unwrap();
        let lease = store.lease(&drop_me.id).unwrap();
        assert_eq!(store.remove(&drop_me.id).unwrap_err().code(), "instance_busy");
        drop(lease);
        store.remove(&drop_me.id).unwrap();
        assert!(!store.instance_dir(&drop_me.id).unwrap().exists());
        assert!(store.get(&keep.id).is_ok());
        assert_eq!(store.remove(&drop_me.id).unwrap_err().code(), "instance_not_found");
    }

    #[test]
    fn lease_is_exclusive_until_dropped() {
        let (_dir, store) = store();
        let created = store.create(input("Leased")).unwrap();
        let lease = store.lease(&created.id).unwrap();
        assert_eq!(lease.id(), &created.id);
        assert_eq!(store.lease(&created.id).unwrap_err().code(), "instance_busy");
        drop(lease);
        assert!(store.lease(&created.id).is_ok());
    }

    #[test]
    fn tampered_game_dir_is_ignored_and_mismatched_ids_are_corrupt() {
        let (_dir, store) = store();
        let created = store.create(input("Safe")).unwrap();
        let manifest = store.instance_dir(&created.id).unwrap().join(MANIFEST);
        let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        raw["gameDir"] = "C:\\Windows".into();
        fs::write(&manifest, raw.to_string()).unwrap();
        assert_eq!(store.get(&created.id).unwrap().game_dir, created.game_dir);
        raw["id"] = "other".into();
        fs::write(&manifest, raw.to_string()).unwrap();
        assert_eq!(store.get(&created.id).unwrap_err().code(), "instance_corrupt");
    }

    #[test]
    fn reads_a_manifest_written_by_the_electron_build() {
        let (_dir, store) = store();
        let dir = store.root().join("default");
        fs::create_dir_all(dir.join("minecraft")).unwrap();
        fs::write(
            dir.join(MANIFEST),
            r#"{
  "name": "Default", "versionId": "1.20.1", "loaderType": "vanilla", "javaPath": "",
  "memoryConfig": { "min": "2G", "max": "4G" }, "icon": "grass", "schemaVersion": 1, "id": "default",
  "gameDir": "C:\\Users\\x\\AppData\\Roaming\\.jifercloud\\instances\\default\\minecraft",
  "createdAt": "2026-05-01T10:00:00.000Z", "lastPlayedAt": null
}"#,
        )
        .unwrap();
        let instance = store.get(&InstanceId::parse("default").unwrap()).unwrap();
        assert_eq!(instance.fields.version_id, "1.20.1");
        assert_eq!(instance.game_dir, dir.join("minecraft"));
    }

    fn legacy_settings(root: &Path) -> LauncherSettings {
        LauncherSettings {
            minecraft_root: root.to_string_lossy().into_owned(),
            selected_version: "1.20.1".into(),
            memory: MemoryConfig::new("1g", "3G"),
            ..LauncherSettings::default()
        }
    }

    #[test]
    fn ensure_default_moves_player_data_and_leaves_shared_binaries() {
        let (dir, store) = store();
        let root = dir.path().join("minecraft");
        for entry in ["saves/world", "mods", "versions/1.20.1", "libraries"] {
            fs::create_dir_all(root.join(entry)).unwrap();
        }
        fs::write(root.join("options.txt"), "lang:es_es").unwrap();

        let result = store.ensure_default(&legacy_settings(&root)).unwrap();
        assert!(result.migrated);
        assert_eq!(result.moved_entries, 3);
        assert_eq!(result.instance.id.as_str(), "default");
        assert_eq!(result.instance.fields.version_id, "1.20.1");
        assert_eq!(result.instance.fields.memory_config, MemoryConfig::new("1G", "3G"));
        let game = &result.instance.game_dir;
        assert!(game.join("saves/world").is_dir() && game.join("mods").is_dir() && game.join("options.txt").is_file());
        assert!(!root.join("saves").exists());
        assert!(root.join("versions/1.20.1").is_dir() && root.join("libraries").is_dir());

        let again = store.ensure_default(&legacy_settings(&root)).unwrap();
        assert!(!again.migrated);
        assert_eq!(again.instance.id, result.instance.id);
    }

    #[test]
    fn ensure_default_prefers_the_active_then_the_latest_instance() {
        let (dir, store) = store();
        let older = store.create(input("Older")).unwrap();
        let newer = store.create(input("Newer")).unwrap();
        store.mark_played(&older.id).unwrap();
        let mut settings = legacy_settings(&dir.path().join("minecraft"));
        assert_eq!(store.ensure_default(&settings).unwrap().instance.id, older.id);
        settings.active_instance_id = newer.id.to_string();
        let result = store.ensure_default(&settings).unwrap();
        assert!(!result.migrated);
        assert_eq!(result.instance.id, newer.id);
    }

    #[test]
    fn ensure_default_never_overwrites_and_tolerates_bad_legacy_values() {
        let (dir, store) = store();
        let root = dir.path().join("minecraft");
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::write(root.join("mods/legacy.jar"), "old").unwrap();
        let target = store.root().join("default/minecraft/mods");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("kept.jar"), "new").unwrap();

        let mut settings = legacy_settings(&root);
        settings.memory = MemoryConfig::new("lots", "more");
        settings.selected_version = "../../evil".into();
        let result = store.ensure_default(&settings).unwrap();
        assert_eq!(result.instance.fields.memory_config, MemoryConfig::new("2G", "4G"));
        assert_eq!(result.instance.fields.version_id, "");
        assert!(target.join("kept.jar").exists());
        assert!(
            root.join("mods/legacy.jar").exists(),
            "conflicting source stays where it was"
        );
    }

    #[test]
    fn ensure_default_handles_a_missing_minecraft_root() {
        let (dir, store) = store();
        let result = store
            .ensure_default(&legacy_settings(&dir.path().join("nowhere")))
            .unwrap();
        assert!(result.migrated);
        assert_eq!(result.moved_entries, 0);
        assert!(result.instance.game_dir.is_dir());
    }
}
