//! Instance manifest model (`instances/<id>/instance.json`, schema version 1) and its validation rules.

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::paths::InstanceId;
use crate::timestamp;
use crate::validate::{self, MemoryConfig, WindowConfig};

pub const SCHEMA_VERSION: u8 = 1;
pub const MAX_NAME_LEN: usize = 64;
pub const DEFAULT_ICON: &str = "grass";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Vanilla,
    Forge,
    Fabric,
    Quilt,
    Neoforge,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub schema_version: u8,
    pub id: InstanceId,
    #[serde(flatten)]
    pub fields: InstanceFields,
    /// Always derived from the id on read; the stored value is informational only.
    pub game_dir: PathBuf,
    pub created_at: String,
    pub last_played_at: Option<String>,
}

impl Instance {
    pub fn last_activity(&self) -> &str {
        self.last_played_at.as_deref().unwrap_or(&self.created_at)
    }
}

/// The user-editable part of an instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceFields {
    pub name: String,
    pub version_id: String,
    pub loader_type: LoaderType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    pub java_path: String,
    pub memory_config: MemoryConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowConfig>,
    pub icon: String,
}

impl InstanceFields {
    pub fn validated(mut self) -> CoreResult<Self> {
        self.name = validate_name(&self.name)?;
        if !validate::is_token(&self.version_id, 0, 64) {
            return Err(invalid("Version id is invalid."));
        }
        if !validate::is_safe_path_text(&self.java_path) {
            return Err(invalid("Java path is invalid."));
        }
        self.java_path = self.java_path.trim().to_owned();
        self.memory_config = self.memory_config.validated().map_err(CoreError::InvalidInstance)?;
        if let Some(window) = self.window {
            self.window = Some(window.validated().map_err(CoreError::InvalidInstance)?);
        }
        if !is_icon(&self.icon) {
            return Err(invalid("Icon is invalid."));
        }
        self.loader_version = match self.loader_type {
            LoaderType::Vanilla => None,
            _ => match self.loader_version {
                Some(version) if validate::is_token(&version, 1, 64) => Some(version),
                _ => return Err(invalid("Loader version is invalid.")),
            },
        };
        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateInstanceInput {
    pub name: String,
    pub version_id: String,
    pub loader_type: LoaderType,
    pub loader_version: Option<String>,
    pub java_path: Option<String>,
    pub memory_config: Option<MemoryConfig>,
    pub window: Option<WindowConfig>,
    pub icon: Option<String>,
}

impl CreateInstanceInput {
    pub fn into_fields(self) -> CoreResult<InstanceFields> {
        InstanceFields {
            name: self.name,
            version_id: self.version_id,
            loader_type: self.loader_type,
            loader_version: self.loader_version,
            java_path: self.java_path.unwrap_or_default(),
            memory_config: self.memory_config.unwrap_or_else(default_memory),
            window: self.window,
            icon: self.icon.unwrap_or_else(|| DEFAULT_ICON.to_owned()),
        }
        .validated()
    }
}

/// Partial edit. `window: null` clears the per-instance override; an absent key keeps it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateInstanceInput {
    pub name: Option<String>,
    pub version_id: Option<String>,
    pub loader_type: Option<LoaderType>,
    pub loader_version: Option<String>,
    pub java_path: Option<String>,
    pub memory_config: Option<MemoryConfig>,
    #[serde(default, deserialize_with = "present")]
    pub window: Option<Option<WindowConfig>>,
    pub icon: Option<String>,
}

impl UpdateInstanceInput {
    pub fn apply_to(self, current: &InstanceFields) -> CoreResult<InstanceFields> {
        let current = current.clone();
        InstanceFields {
            name: self.name.unwrap_or(current.name),
            version_id: self.version_id.unwrap_or(current.version_id),
            loader_type: self.loader_type.unwrap_or(current.loader_type),
            loader_version: self.loader_version.or(current.loader_version),
            java_path: self.java_path.unwrap_or(current.java_path),
            memory_config: self.memory_config.unwrap_or(current.memory_config),
            window: self.window.unwrap_or(current.window),
            icon: self.icon.unwrap_or(current.icon),
        }
        .validated()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloneInstanceInput {
    pub source_id: String,
    pub name: String,
    pub copy_worlds: bool,
}

/// Shape of a manifest on disk. `gameDir` and unknown keys are ignored on purpose.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredManifest {
    schema_version: u8,
    id: String,
    #[serde(flatten)]
    fields: InstanceFields,
    created_at: String,
    last_played_at: Option<String>,
}

pub(crate) fn parse_manifest(id: &InstanceId, game_dir: PathBuf, bytes: &[u8]) -> CoreResult<Instance> {
    let corrupt = |reason: String| CoreError::InstanceCorrupt {
        id: id.to_string(),
        reason,
    };
    let stored: StoredManifest = serde_json::from_slice(bytes).map_err(|error| corrupt(error.to_string()))?;
    if stored.schema_version != SCHEMA_VERSION || stored.id != id.as_str() {
        return Err(corrupt("unexpected schema or id".to_owned()));
    }
    let fields = stored.fields.validated().map_err(|error| corrupt(error.to_string()))?;
    let timestamps_ok = timestamp::is_valid_iso(&stored.created_at)
        && stored.last_played_at.as_deref().is_none_or(timestamp::is_valid_iso);
    if !timestamps_ok {
        return Err(corrupt("Timestamp is invalid.".to_owned()));
    }
    Ok(Instance {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        fields,
        game_dir,
        created_at: stored.created_at,
        last_played_at: stored.last_played_at,
    })
}

pub fn default_memory() -> MemoryConfig {
    MemoryConfig::new("2G", "4G")
}

pub(crate) fn validate_name(raw: &str) -> CoreResult<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_LEN {
        return Err(invalid(&format!("Instance name must be 1-{MAX_NAME_LEN} characters.")));
    }
    Ok(name.to_owned())
}

fn is_icon(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidInstance(message.to_owned())
}

#[expect(clippy::option_option, reason = "absent keeps the value, null clears it")]
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> InstanceFields {
        InstanceFields {
            name: "  Survival  ".to_owned(),
            version_id: "1.20.1".to_owned(),
            loader_type: LoaderType::Vanilla,
            loader_version: Some("0.15.0".to_owned()),
            java_path: " C:\\Java\\bin\\javaw.exe ".to_owned(),
            memory_config: default_memory(),
            window: None,
            icon: DEFAULT_ICON.to_owned(),
        }
    }

    #[test]
    fn normalizes_and_drops_the_loader_version_for_vanilla() {
        let valid = fields().validated().unwrap();
        assert_eq!(valid.name, "Survival");
        assert_eq!(valid.java_path, "C:\\Java\\bin\\javaw.exe");
        assert_eq!(valid.loader_version, None);
    }

    #[test]
    fn modded_loaders_require_a_loader_version() {
        let mut modded = fields();
        modded.loader_type = LoaderType::Fabric;
        assert_eq!(
            modded.clone().validated().unwrap().loader_version.as_deref(),
            Some("0.15.0")
        );
        modded.loader_version = None;
        assert_eq!(modded.validated().unwrap_err().code(), "invalid_instance");
    }

    #[test]
    fn rejects_invalid_fields() {
        let cases: [fn(&mut InstanceFields); 7] = [
            |f| f.name = "   ".to_owned(),
            |f| f.name = "x".repeat(65),
            |f| f.version_id = "../1.20".to_owned(),
            |f| f.java_path = "C:\\java\0".to_owned(),
            |f| f.memory_config = MemoryConfig::new("8G", "2G"),
            |f| f.icon = "Grass!".to_owned(),
            |f| {
                f.window = Some(WindowConfig {
                    width: 100,
                    height: 100,
                    fullscreen: false,
                });
            },
        ];
        for mutate in cases {
            let mut candidate = fields();
            mutate(&mut candidate);
            assert_eq!(candidate.validated().unwrap_err().code(), "invalid_instance");
        }
    }

    #[test]
    fn update_distinguishes_absent_and_null_window() {
        let mut current = fields().validated().unwrap();
        current.window = Some(WindowConfig {
            width: 800,
            height: 600,
            fullscreen: false,
        });
        let keep: UpdateInstanceInput = serde_json::from_str(r#"{"name":"Renamed"}"#).unwrap();
        let clear: UpdateInstanceInput = serde_json::from_str(r#"{"window":null}"#).unwrap();
        assert_eq!(keep.apply_to(&current).unwrap().window, current.window);
        assert_eq!(clear.apply_to(&current).unwrap().window, None);
    }

    #[test]
    fn serializes_in_the_electron_shape() {
        let instance = Instance {
            schema_version: 1,
            id: InstanceId::parse("default").unwrap(),
            fields: fields().validated().unwrap(),
            game_dir: PathBuf::from("/data/instances/default/minecraft"),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            last_played_at: None,
        };
        let json = serde_json::to_value(&instance).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["loaderType"], "vanilla");
        assert_eq!(json["memoryConfig"]["max"], "4G");
        assert!(json["lastPlayedAt"].is_null());
        assert!(json.get("loaderVersion").is_none());
        assert!(json.get("window").is_none());
    }
}
