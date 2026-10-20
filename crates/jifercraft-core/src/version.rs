// Mojang's version manifest parser and rule engine.
// Filtering libraries by OS, arch, and legacy Mojang heuristics without pulling in half of crates.io.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    pub main_class: String,
    /// Pre-1.13 single string of game arguments.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub downloads: Option<VersionDownloads>,
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub logging: Option<Logging>,
    #[serde(default)]
    pub inherits_from: Option<String>,
}

impl VersionJson {
    /// Java major the version was built for; versions before the field existed run on Java 8.
    pub fn java_major(&self) -> u32 {
        self.java_version.as_ref().map_or(8, |java| java.major_version)
    }

    pub fn assets_id(&self) -> &str {
        self.asset_index
            .as_ref()
            .map(|index| index.id.as_str())
            .or(self.assets.as_deref())
            .unwrap_or("legacy")
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn as_slice(&self) -> &[String] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    /// A regex over the OS version. Only Mojang's Windows 10 heuristics use it; both Windows 10 and 11 report
    /// `10.x`, so it is treated as matching rather than pulling in a regex engine.
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub extract: Option<Extract>,
    /// Maven repository base for libraries without explicit downloads (mod loaders).
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// The platform and feature flags rules are evaluated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Environment {
    pub os: &'static str,
    pub arch: &'static str,
    pub custom_resolution: bool,
}

impl Environment {
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "macos" => "osx",
            "windows" => "windows",
            _ => "linux",
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "aarch64" => "arm64",
            _ => "x86_64",
        };
        Self {
            os,
            arch,
            custom_resolution: false,
        }
    }

    /// Value substituted for `${arch}` in legacy native classifiers.
    pub fn arch_bits(&self) -> &'static str {
        if self.arch == "x86" { "32" } else { "64" }
    }
}

// Mojang's rule engine has upside-down logic:
// If the rules list is empty -> ALLOW.
// If there ARE rules -> start with FALSE and let the last matching rule flip the verdict.
// Don't shoot the messenger, we didn't write this spec.
pub fn rules_allow(rules: &[Rule], env: &Environment) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules.iter().fold(false, |allowed, rule| {
        if rule_matches(rule, env) {
            rule.action == RuleAction::Allow
        } else {
            allowed
        }
    })
}

fn rule_matches(rule: &Rule, env: &Environment) -> bool {
    let os_matches = rule.os.as_ref().is_none_or(|os| {
        os.name.as_deref().is_none_or(|name| name == env.os) && os.arch.as_deref().is_none_or(|arch| arch == env.arch)
    });
    let features_match = rule.features.as_ref().is_none_or(|features| {
        features.iter().all(|(name, wanted)| {
            let enabled = name == "has_custom_resolution" && env.custom_resolution;
            enabled == *wanted
        })
    });
    os_matches && features_match
}

/// Flattens modern arguments, keeping only those whose rules allow them.
pub fn resolve_arguments<'a>(arguments: &'a [Argument], env: &Environment) -> Vec<&'a str> {
    arguments
        .iter()
        .flat_map(|argument| match argument {
            Argument::Plain(value) => std::slice::from_ref(value),
            Argument::Conditional { rules, value } if rules_allow(rules, env) => value.as_slice(),
            Argument::Conditional { .. } => &[],
        })
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: Environment = Environment {
        os: "windows",
        arch: "x86_64",
        custom_resolution: false,
    };
    const LINUX: Environment = Environment {
        os: "linux",
        arch: "x86_64",
        custom_resolution: false,
    };

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_rules_allow_everything() {
        assert!(rules_allow(&[], &WINDOWS));
    }

    #[test]
    fn last_matching_rule_wins() {
        let except_osx = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(rules_allow(&except_osx, &WINDOWS));
        assert!(!rules_allow(&except_osx, &Environment { os: "osx", ..WINDOWS }));
        let only_linux = rules(r#"[{"action":"allow","os":{"name":"linux"}}]"#);
        assert!(!rules_allow(&only_linux, &WINDOWS));
        assert!(rules_allow(&only_linux, &LINUX));
    }

    #[test]
    fn arch_and_feature_rules() {
        let x86_only = rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#);
        assert!(!rules_allow(&x86_only, &WINDOWS));
        let demo = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!rules_allow(&demo, &WINDOWS));
        let resolution = rules(r#"[{"action":"allow","features":{"has_custom_resolution":true}}]"#);
        assert!(!rules_allow(&resolution, &WINDOWS));
        assert!(rules_allow(
            &resolution,
            &Environment {
                custom_resolution: true,
                ..WINDOWS
            }
        ));
    }

    #[test]
    fn resolves_mixed_argument_lists() {
        let arguments: Vec<Argument> = serde_json::from_str(
            r#"["--username", "${auth_player_name}",
                {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"},
                {"rules":[{"action":"allow","os":{"name":"windows"}}],"value":["-XX:HeapDumpPath=x.hprof"]},
                {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":"-XstartOnFirstThread"}]"#,
        )
        .unwrap();
        assert_eq!(
            resolve_arguments(&arguments, &WINDOWS),
            ["--username", "${auth_player_name}", "-XX:HeapDumpPath=x.hprof"]
        );
    }

    #[test]
    fn java_major_defaults_to_eight() {
        let legacy: VersionJson =
            serde_json::from_str(r#"{"id":"1.8.9","type":"release","mainClass":"net.minecraft.client.main.Main"}"#)
                .unwrap();
        assert_eq!(legacy.java_major(), 8);
        assert_eq!(legacy.assets_id(), "legacy");
    }
}
