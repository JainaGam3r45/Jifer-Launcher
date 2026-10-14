use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

pub const DATA_DIR_NAME: &str = ".jifercloud";

/// Root of every file the launcher owns: `<appData>/.jifercloud`.
#[derive(Debug, Clone)]
pub struct LauncherPaths {
    root: PathBuf,
}

impl LauncherPaths {
    /// `app_data` is the platform roaming data directory (`%APPDATA%` on Windows).
    pub fn from_app_data(app_data: impl AsRef<Path>) -> Self {
        Self::at(app_data.as_ref().join(DATA_DIR_NAME))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn legacy_session_file(&self) -> PathBuf {
        self.root.join("session.json")
    }

    pub fn default_minecraft_root(&self) -> PathBuf {
        self.root.join("minecraft")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtimes")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }
}

/// Validated instance id: lowercase alphanumerics and inner dashes, 1-64 chars.
/// The pattern alone rules out separators, dots and drive letters, so it cannot escape the instances directory.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct InstanceId(String);

impl InstanceId {
    pub fn parse(value: &str) -> CoreResult<Self> {
        if Self::is_valid(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(CoreError::InvalidInstanceId)
        }
    }

    pub fn is_valid(value: &str) -> bool {
        let bytes = value.as_bytes();
        let allowed = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        match (bytes.first(), bytes.last()) {
            (Some(first), Some(last)) => {
                bytes.len() <= 64 && allowed(first) && allowed(last) && bytes.iter().all(|b| allowed(b) || *b == b'-')
            }
            _ => false,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for InstanceId {
    type Error = CoreError;

    fn try_from(value: String) -> CoreResult<Self> {
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(CoreError::InvalidInstanceId)
        }
    }
}

impl From<InstanceId> for String {
    fn from(value: InstanceId) -> Self {
        value.0
    }
}

/// Lexically checks that `target` is strictly below `root` (the root itself is rejected).
pub fn assert_inside(root: &Path, target: &Path) -> CoreResult<PathBuf> {
    let root = normalize(root);
    let target = normalize(target);
    match target.strip_prefix(&root) {
        Ok(rest) if rest.components().next().is_some() => Ok(target),
        _ => Err(CoreError::PathOutsideInstances),
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Comprueba si una ruta está alojada dentro de una carpeta sincronizada por `OneDrive`.
///
/// Si Minecraft intenta guardar mundos o escribir librerías dentro de `OneDrive`, el
/// sincronizador de Microsoft intentará subir chunks mientras el juego los modifica,
/// lo cual termina en mundos corrompidos, conflictos de sincronización y lágrimas de jugadores.
pub fn is_onedrive_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_ascii_lowercase();
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(onedrive_root) = std::env::var_os(var) {
            let root_str = onedrive_root.to_string_lossy().to_ascii_lowercase();
            if !root_str.is_empty() && path_str.starts_with(&root_str) {
                return true;
            }
        }
    }
    path.components().any(|component| match component {
        Component::Normal(name) => {
            let lower = name.to_string_lossy().to_ascii_lowercase();
            lower == "onedrive" || lower.starts_with("onedrive ") || lower.starts_with("onedrive-")
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slug_and_uuid_ids() {
        for id in ["default", "a", "0", "my-pack-2", "9b2f6a8e-3c1d-4e5f-8a7b-0c1d2e3f4a5b"] {
            assert!(InstanceId::is_valid(id), "{id}");
        }
        assert!(InstanceId::is_valid(&"a".repeat(64)));
    }

    #[test]
    fn rejects_hostile_or_malformed_ids() {
        for id in [
            "",
            "..",
            ".",
            "../x",
            "a/b",
            "a\\b",
            "C:",
            "-a",
            "a-",
            "A",
            "a b",
            "a.b",
            "a\0",
            "caf\u{e9}",
        ] {
            assert!(!InstanceId::is_valid(id), "{id:?}");
        }
        assert!(!InstanceId::is_valid(&"a".repeat(65)));
    }

    #[test]
    fn deserializes_only_valid_ids() {
        assert!(serde_json::from_str::<InstanceId>("\"default\"").is_ok());
        assert!(serde_json::from_str::<InstanceId>("\"../etc\"").is_err());
    }

    #[test]
    fn assert_inside_rejects_root_parents_and_siblings() {
        let root = Path::new("/data/instances");
        assert!(assert_inside(root, Path::new("/data/instances/a")).is_ok());
        assert!(assert_inside(root, Path::new("/data/instances/a/minecraft")).is_ok());
        assert!(assert_inside(root, Path::new("/data/instances")).is_err());
        assert!(assert_inside(root, Path::new("/data/instances/.")).is_err());
        assert!(assert_inside(root, Path::new("/data")).is_err());
        assert!(assert_inside(root, Path::new("/data/instances/../other")).is_err());
        assert!(assert_inside(root, Path::new("/data/instances-evil/a")).is_err());
    }

    #[test]
    fn detects_onedrive_paths() {
        assert!(is_onedrive_path(Path::new("C:\\Users\\User\\OneDrive\\Minecraft")));
        assert!(is_onedrive_path(Path::new("D:\\OneDrive - Empresa\\.jifercloud")));
        assert!(!is_onedrive_path(Path::new(
            "C:\\Users\\User\\AppData\\Roaming\\.jifercloud"
        )));
    }
}
