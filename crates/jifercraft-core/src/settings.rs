//! `settings.json`: launcher-wide preferences. Field names and defaults match the Electron build.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::paths::{InstanceId, LauncherPaths};
use crate::store;
use crate::validate::{self, MemoryConfig, WindowConfig, lenient};

pub const MAX_OFFLINE_ACCOUNTS: usize = 6;
const MAX_SEARCH_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherSettings {
    pub active_instance_id: String,
    pub minecraft_root: String,
    #[serde(deserialize_with = "lenient")]
    pub window: WindowConfig,
    #[serde(deserialize_with = "lenient")]
    pub user: UserSettings,
    #[serde(deserialize_with = "lenient")]
    pub ui: UiSettings,
    /// Legacy: moved to `Instance.versionId`. Read once by the Default-instance migration.
    pub selected_version: String,
    /// Legacy: moved to `Instance.javaPath`.
    pub java_path: String,
    /// Legacy: moved to `Instance.memoryConfig`.
    #[serde(deserialize_with = "lenient")]
    pub memory: MemoryConfig,
    /// Legacy: derived from `Instance.lastPlayedAt`.
    #[serde(deserialize_with = "lenient")]
    pub history: History,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fullscreen: false,
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self::new("2G", "4G")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserSettings {
    pub last_offline_username: String,
    pub remember_offline_session: bool,
    pub offline_accounts: Vec<String>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            last_offline_username: String::new(),
            remember_offline_session: true,
            offline_accounts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub language: Language,
    pub version_filter: VersionFilter,
    pub version_search: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Es,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionFilter {
    #[default]
    Release,
    Snapshot,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub played_versions: Vec<String>,
}

/// Partial update coming from the UI. Every field is validated before anything is written.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub active_instance_id: Option<String>,
    pub minecraft_root: Option<String>,
    pub window: Option<WindowConfig>,
    pub user: Option<UserPatch>,
    pub ui: Option<UiPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserPatch {
    pub last_offline_username: Option<String>,
    pub remember_offline_session: Option<bool>,
    pub offline_accounts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPatch {
    pub language: Option<Language>,
    pub version_filter: Option<VersionFilter>,
    pub version_search: Option<String>,
}

impl LauncherSettings {
    pub fn defaults_for(paths: &LauncherPaths) -> Self {
        Self {
            minecraft_root: paths.default_minecraft_root().to_string_lossy().into_owned(),
            ..Self::default()
        }
    }

    pub fn minecraft_root(&self) -> PathBuf {
        PathBuf::from(&self.minecraft_root)
    }

    pub fn apply(&self, patch: SettingsPatch) -> CoreResult<Self> {
        let mut next = self.clone();
        if let Some(id) = patch.active_instance_id {
            next.active_instance_id = InstanceId::parse(&id)?.into();
        }
        if let Some(root) = patch.minecraft_root {
            let root = root.trim();
            if root.is_empty() || !validate::is_safe_path_text(root) || !Path::new(root).is_absolute() {
                return Err(invalid("Minecraft folder must be an absolute path."));
            }
            root.clone_into(&mut next.minecraft_root);
        }
        if let Some(window) = patch.window {
            next.window = window.validated().map_err(CoreError::InvalidSettings)?;
        }
        if let Some(user) = patch.user {
            apply_user(&mut next.user, user)?;
        }
        if let Some(ui) = patch.ui {
            next.ui.language = ui.language.unwrap_or(next.ui.language);
            next.ui.version_filter = ui.version_filter.unwrap_or(next.ui.version_filter);
            if let Some(search) = ui.version_search {
                if search.chars().count() > MAX_SEARCH_LEN {
                    return Err(invalid("Version search is too long."));
                }
                next.ui.version_search = search;
            }
        }
        Ok(next)
    }
}

fn apply_user(user: &mut UserSettings, patch: UserPatch) -> CoreResult<()> {
    if let Some(name) = patch.last_offline_username {
        if !name.is_empty() && !validate::is_offline_username(&name) {
            return Err(invalid("Offline username is invalid."));
        }
        user.last_offline_username = name;
    }
    if let Some(remember) = patch.remember_offline_session {
        user.remember_offline_session = remember;
    }
    if let Some(accounts) = patch.offline_accounts {
        if accounts.len() > MAX_OFFLINE_ACCOUNTS || !accounts.iter().all(|name| validate::is_offline_username(name)) {
            return Err(invalid("Offline account list is invalid."));
        }
        user.offline_accounts = accounts;
    }
    Ok(())
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidSettings(message.to_owned())
}

/// Cached, write-through access to `settings.json`.
#[derive(Debug)]
pub struct SettingsStore {
    paths: LauncherPaths,
    cached: Mutex<Option<LauncherSettings>>,
}

impl SettingsStore {
    pub fn new(paths: LauncherPaths) -> Self {
        Self {
            paths,
            cached: Mutex::new(None),
        }
    }

    pub fn get(&self) -> CoreResult<LauncherSettings> {
        let mut cached = self.lock();
        if let Some(settings) = cached.as_ref() {
            return Ok(settings.clone());
        }
        let settings = self.load()?;
        *cached = Some(settings.clone());
        Ok(settings)
    }

    pub fn update(&self, patch: SettingsPatch) -> CoreResult<LauncherSettings> {
        self.modify(|current| current.apply(patch))
    }

    /// Internal mutations (migration bookkeeping, played history) that bypass UI validation.
    pub fn modify(
        &self,
        change: impl FnOnce(&LauncherSettings) -> CoreResult<LauncherSettings>,
    ) -> CoreResult<LauncherSettings> {
        let mut cached = self.lock();
        let current = match cached.as_ref() {
            Some(settings) => settings.clone(),
            None => self.load()?,
        };
        let next = change(&current)?;
        store::write_json_atomic(&self.paths.settings_file(), &next)?;
        *cached = Some(next.clone());
        Ok(next)
    }

    fn load(&self) -> CoreResult<LauncherSettings> {
        let path = self.paths.settings_file();
        store::discard_stale_temp(&path);
        let loaded = match store::read_json::<LauncherSettings>(&path) {
            Ok(Some(settings)) => Some(settings),
            Ok(None) => None,
            Err(error @ CoreError::Json { .. }) => {
                tracing::warn!(%error, "settings.json is unreadable; keeping a backup and starting from defaults");
                let _ = std::fs::rename(&path, path.with_extension("json.bak"));
                None
            }
            Err(error) => return Err(error),
        };
        let mut settings = if let Some(settings) = loaded {
            settings
        } else {
            let defaults = LauncherSettings::defaults_for(&self.paths);
            store::write_json_atomic(&path, &defaults)?;
            defaults
        };
        if settings.minecraft_root.trim().is_empty() {
            settings.minecraft_root = LauncherSettings::defaults_for(&self.paths).minecraft_root;
        }
        Ok(settings)
    }

    fn lock(&self) -> MutexGuard<'_, Option<LauncherSettings>> {
        // The guarded value is a plain cache; a panic elsewhere cannot leave it half-written.
        self.cached.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(LauncherPaths::at(dir.path()));
        (dir, store)
    }

    #[test]
    fn creates_defaults_on_first_run() {
        let (dir, store) = store();
        let settings = store.get().unwrap();
        assert_eq!(settings.ui.language, Language::Es);
        assert_eq!(settings.memory, MemoryConfig::new("2G", "4G"));
        assert_eq!(
            settings.window,
            WindowConfig {
                width: 1280,
                height: 720,
                fullscreen: false
            }
        );
        assert!(settings.user.remember_offline_session);
        assert_eq!(PathBuf::from(&settings.minecraft_root), dir.path().join("minecraft"));
        assert!(dir.path().join("settings.json").exists());
    }

    #[test]
    fn reads_electron_settings_and_fills_missing_fields() {
        let (dir, store) = store();
        fs::write(
            dir.path().join("settings.json"),
            r#"{"activeInstanceId":"default","selectedVersion":"1.20.1","minecraftRoot":"C:\\mc","javaPath":"",
               "memory":{"min":"1G","max":"4G"},"user":{"lastOfflineUsername":"Steve"},"ui":{"language":"en"},
               "window":{"width":"wide"}}"#,
        )
        .unwrap();
        let settings = store.get().unwrap();
        assert_eq!(settings.active_instance_id, "default");
        assert_eq!(settings.selected_version, "1.20.1");
        assert_eq!(settings.minecraft_root, "C:\\mc");
        assert_eq!(settings.memory, MemoryConfig::new("1G", "4G"));
        assert_eq!(settings.user.last_offline_username, "Steve");
        assert!(settings.user.remember_offline_session);
        assert_eq!(settings.ui.language, Language::En);
        assert_eq!(settings.ui.version_filter, VersionFilter::Release);
        assert_eq!(
            settings.window,
            WindowConfig::default(),
            "a malformed group falls back alone"
        );
    }

    #[test]
    fn backs_up_an_unreadable_file() {
        let (dir, store) = store();
        fs::write(dir.path().join("settings.json"), "{ broken").unwrap();
        assert_eq!(store.get().unwrap().ui.language, Language::Es);
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.json.bak")).unwrap(),
            "{ broken"
        );
    }

    #[test]
    fn applies_a_valid_patch_and_persists_it() {
        let (dir, store) = store();
        let patch: SettingsPatch =
            serde_json::from_str(r#"{"ui":{"language":"en"},"window":{"width":1600,"height":900,"fullscreen":true}}"#)
                .unwrap();
        let next = store.update(patch).unwrap();
        assert_eq!(next.ui.language, Language::En);
        let reread = SettingsStore::new(LauncherPaths::at(dir.path())).get().unwrap();
        assert_eq!(reread, next);
    }

    #[test]
    fn rejects_invalid_patches_without_writing() {
        let (_dir, store) = store();
        let before = store.get().unwrap();
        for raw in [
            r#"{"activeInstanceId":"../x"}"#,
            r#"{"minecraftRoot":"relative/path"}"#,
            r#"{"minecraftRoot":""}"#,
            r#"{"window":{"width":10,"height":10,"fullscreen":false}}"#,
            r#"{"user":{"lastOfflineUsername":"no spaces"}}"#,
            r#"{"user":{"offlineAccounts":["a1","b2","c3","d4","e5","f6","g7"]}}"#,
        ] {
            let patch: SettingsPatch = serde_json::from_str(raw).unwrap();
            let error = store.update(patch).unwrap_err();
            assert!(
                matches!(error.code(), "invalid_settings" | "invalid_instance_id"),
                "{raw}"
            );
        }
        assert_eq!(store.get().unwrap(), before);
    }

    #[test]
    fn patch_rejects_unknown_or_mistyped_fields() {
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"memory":{"max":"64G"}}"#).is_err());
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"ui":{"language":"fr"}}"#).is_err());
    }
}
