// Assembles the exact java command line for launching Minecraft, bridging the modern 1.13+
// argument arrays with the ancient 1.8.9 flat minecraftArguments string without losing our sanity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::install::{InstallPlan, legacy_assets_dir};
use crate::validate::{MemoryConfig, WindowConfig};
use crate::version::{Environment, VersionJson, resolve_arguments};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchAuth {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    /// `msa` for Microsoft accounts, `legacy` for offline play.
    pub user_type: String,
    pub xuid: Option<String>,
    pub client_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchSpec<'a> {
    pub version: &'a VersionJson,
    pub plan: &'a InstallPlan,
    pub java: &'a Path,
    pub minecraft_root: &'a Path,
    pub game_dir: &'a Path,
    pub auth: &'a LaunchAuth,
    pub memory: &'a MemoryConfig,
    pub window: Option<WindowConfig>,
    pub env: Environment,
    pub launcher_name: &'a str,
    pub launcher_version: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    secret: String,
}

impl LaunchCommand {
    pub fn new(program: PathBuf, args: Vec<String>, cwd: PathBuf) -> Self {
        Self {
            program,
            args,
            cwd,
            secret: String::new(),
        }
    }

    /// Arguments safe to log: the session token is masked.
    pub fn redacted_args(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|arg| {
                if self.secret.is_empty() {
                    arg.clone()
                } else {
                    arg.replace(&self.secret, "********")
                }
            })
            .collect()
    }
}

pub fn build_command(spec: &LaunchSpec<'_>) -> LaunchCommand {
    let window = spec.window.filter(|window| !window.fullscreen);
    let env = Environment {
        custom_resolution: window.is_some(),
        ..spec.env
    };
    let values = placeholders(spec, window);
    let substitute = |arg: &str| substitute(arg, &values);

    let mut args = vec![format!("-Xms{}", spec.memory.min), format!("-Xmx{}", spec.memory.max)];
    // Log4Shell: old clients ship vulnerable log4j; the flag is harmless on patched ones.
    args.push("-Dlog4j2.formatMsgNoLookups=true".to_owned());
    if let Some(logging) = &spec.plan.logging_argument {
        args.push(logging.clone());
    }

    let modern = spec.version.arguments.as_ref();
    match modern {
        Some(arguments) if !arguments.jvm.is_empty() => {
            args.extend(resolve_arguments(&arguments.jvm, &env).into_iter().map(substitute));
        }
        _ => args.extend(
            [
                "-Djava.library.path=${natives_directory}",
                "-Dminecraft.launcher.brand=${launcher_name}",
                "-Dminecraft.launcher.version=${launcher_version}",
                "-cp",
                "${classpath}",
            ]
            .into_iter()
            .map(substitute),
        ),
    }

    args.push(spec.version.main_class.clone());

    match (modern, &spec.version.minecraft_arguments) {
        (Some(arguments), _) if !arguments.game.is_empty() => {
            args.extend(resolve_arguments(&arguments.game, &env).into_iter().map(substitute));
        }
        (_, Some(legacy)) => {
            args.extend(legacy.split_whitespace().map(substitute));
            if let Some(window) = window {
                args.extend([
                    "--width".to_owned(),
                    window.width.to_string(),
                    "--height".to_owned(),
                    window.height.to_string(),
                ]);
            }
        }
        _ => {}
    }
    if spec.window.is_some_and(|window| window.fullscreen) {
        args.push("--fullscreen".to_owned());
    }

    LaunchCommand {
        program: spec.java.to_path_buf(),
        args,
        cwd: spec.game_dir.to_path_buf(),
        secret: spec.auth.access_token.clone(),
    }
}

fn placeholders(spec: &LaunchSpec<'_>, window: Option<WindowConfig>) -> HashMap<&'static str, String> {
    let separator = if spec.env.os == "windows" { ";" } else { ":" };
    let classpath = spec
        .plan
        .classpath
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(separator);
    let assets_root = spec.minecraft_root.join("assets");
    let assets_id = spec.version.assets_id();
    let uuid = spec.auth.uuid.replace('-', "");
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let (width, height) = window.map_or((String::new(), String::new()), |w| {
        (w.width.to_string(), w.height.to_string())
    });

    HashMap::from([
        ("auth_player_name", spec.auth.username.clone()),
        ("auth_uuid", uuid.clone()),
        ("auth_access_token", spec.auth.access_token.clone()),
        ("auth_session", format!("token:{}:{uuid}", spec.auth.access_token)),
        ("auth_xuid", spec.auth.xuid.clone().unwrap_or_else(|| "0".to_owned())),
        (
            "clientid",
            spec.auth.client_id.clone().unwrap_or_else(|| "0".to_owned()),
        ),
        ("user_type", spec.auth.user_type.clone()),
        ("user_properties", "{}".to_owned()),
        ("version_name", spec.version.id.clone()),
        (
            "version_type",
            if spec.version.kind.is_empty() {
                "release".to_owned()
            } else {
                spec.version.kind.clone()
            },
        ),
        ("game_directory", path(spec.game_dir)),
        ("assets_root", path(&assets_root)),
        ("game_assets", path(&legacy_assets_dir(spec.minecraft_root, assets_id))),
        ("assets_index_name", assets_id.to_owned()),
        ("natives_directory", path(&spec.plan.natives_dir)),
        ("library_directory", path(&spec.minecraft_root.join("libraries"))),
        ("classpath_separator", separator.to_owned()),
        ("classpath", classpath),
        ("launcher_name", spec.launcher_name.to_owned()),
        ("launcher_version", spec.launcher_version.to_owned()),
        ("resolution_width", width),
        ("resolution_height", height),
    ])
}

/// Replaces every known `${key}`; unknown keys are left verbatim, as Mojang's launcher does.
fn substitute(arg: &str, values: &HashMap<&'static str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start + 2..].find('}') else { break };
        let key = &rest[start + 2..start + 2 + len];
        out.push_str(&rest[..start]);
        if let Some(value) = values.get(key) {
            out.push_str(value);
        } else {
            out.push_str(&rest[start..start + 3 + len]);
        }
        rest = &rest[start + 3 + len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: Environment = Environment {
        os: "windows",
        arch: "x86_64",
        custom_resolution: false,
    };

    const MODERN: &str = r#"{"id": "1.20.1", "type": "release", "mainClass": "net.minecraft.client.main.Main", "assets": "5",
      "assetIndex": {"id": "5", "sha1": "x", "size": 1, "url": "https://x.test"},
      "arguments": {"game": ["--username", "${auth_player_name}", "--version", "${version_name}", "--gameDir", "${game_directory}", "--assetsDir", "${assets_root}", "--assetIndex", "${assets_index_name}", "--uuid", "${auth_uuid}", "--accessToken", "${auth_access_token}", "--clientId", "${clientid}", "--xuid", "${auth_xuid}", "--userType", "${user_type}", "--versionType", "${version_type}", {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}, {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}], "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]}, {"rules": [{"action": "allow", "features": {"has_quick_plays_support": true}}], "value": ["--quickPlayPath", "${quickPlayPath}"]}],
      "jvm": [{"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": ["-XstartOnFirstThread"]}, {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"}, {"rules": [{"action": "allow", "os": {"arch": "x86"}}], "value": "-Xss1M"}, "-Djava.library.path=${natives_directory}", "-Djna.tmpdir=${natives_directory}", "-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}", "-Dio.netty.native.workdir=${natives_directory}", "-Dminecraft.launcher.brand=${launcher_name}", "-Dminecraft.launcher.version=${launcher_version}", "-cp", "${classpath}"]},
      "javaVersion": {"component": "java-runtime-gamma", "majorVersion": 17}}"#;

    const LEGACY: &str = r#"{"id": "1.8.9", "type": "release", "mainClass": "net.minecraft.client.main.Main", "assets": "1.8",
      "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userProperties ${user_properties} --userType ${user_type}"}"#;

    fn auth() -> LaunchAuth {
        LaunchAuth {
            username: "Steve".into(),
            uuid: "4566e69f-c907-48ee-8d71-d7ba5aa00d20".into(),
            access_token: "secret-token".into(),
            user_type: "msa".into(),
            xuid: Some("2535".into()),
            client_id: None,
        }
    }

    fn plan(root: &Path, id: &str) -> InstallPlan {
        InstallPlan {
            jobs: Vec::new(),
            classpath: vec![
                root.join("libraries").join("a.jar"),
                root.join("versions").join(id).join(format!("{id}.jar")),
            ],
            native_archives: Vec::new(),
            natives_dir: root.join("natives").join(id),
            client_jar: root.join("versions").join(id).join(format!("{id}.jar")),
            logging_argument: Some("-Dlog4j.configurationFile=client.xml".into()),
        }
    }

    fn command(json: &str, window: Option<WindowConfig>) -> (LaunchCommand, PathBuf) {
        let version: VersionJson = serde_json::from_str(json).unwrap();
        let root = PathBuf::from("C:\\mc");
        let plan = plan(&root, &version.id);
        let game = root.join("instances").join("default");
        let auth = auth();
        let memory = MemoryConfig::new("2G", "4G");
        let java = PathBuf::from("C:\\java\\bin\\javaw.exe");
        let command = build_command(&LaunchSpec {
            version: &version,
            plan: &plan,
            java: &java,
            minecraft_root: &root,
            game_dir: &game,
            auth: &auth,
            memory: &memory,
            window,
            env: WINDOWS,
            launcher_name: "jifercraft-launcher",
            launcher_version: "0.2.0",
        });
        (command, root)
    }

    fn p(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn modern_versions_follow_the_argument_rules() {
        let (command, root) = command(MODERN, None);
        let natives = p(&root.join("natives").join("1.20.1"));
        let classpath = format!(
            "{};{}",
            p(&root.join("libraries").join("a.jar")),
            p(&root.join("versions").join("1.20.1").join("1.20.1.jar"))
        );
        let expected: Vec<String> = [
            "-Xms2G",
            "-Xmx4G",
            "-Dlog4j2.formatMsgNoLookups=true",
            "-Dlog4j.configurationFile=client.xml",
            "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump",
            &format!("-Djava.library.path={natives}"),
            &format!("-Djna.tmpdir={natives}"),
            &format!("-Dorg.lwjgl.system.SharedLibraryExtractPath={natives}"),
            &format!("-Dio.netty.native.workdir={natives}"),
            "-Dminecraft.launcher.brand=jifercraft-launcher",
            "-Dminecraft.launcher.version=0.2.0",
            "-cp",
            &classpath,
            "net.minecraft.client.main.Main",
            "--username",
            "Steve",
            "--version",
            "1.20.1",
            "--gameDir",
            &p(&root.join("instances").join("default")),
            "--assetsDir",
            &p(&root.join("assets")),
            "--assetIndex",
            "5",
            "--uuid",
            "4566e69fc90748ee8d71d7ba5aa00d20",
            "--accessToken",
            "secret-token",
            "--clientId",
            "0",
            "--xuid",
            "2535",
            "--userType",
            "msa",
            "--versionType",
            "release",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(command.args, expected);
        assert_eq!(command.cwd, root.join("instances").join("default"));
    }

    #[test]
    fn modern_custom_resolution_and_fullscreen() {
        let window = WindowConfig {
            width: 1600,
            height: 900,
            fullscreen: false,
        };
        let (sized, _) = command(MODERN, Some(window));
        assert!(
            sized
                .args
                .ends_with(&["--width", "1600", "--height", "900"].map(str::to_owned))
        );
        let (full, _) = command(
            MODERN,
            Some(WindowConfig {
                fullscreen: true,
                ..window
            }),
        );
        assert_eq!(full.args.last().map(String::as_str), Some("--fullscreen"));
        assert!(!full.args.iter().any(|arg| arg == "--width"));
    }

    #[test]
    fn legacy_versions_get_default_jvm_arguments_and_resolution() {
        let window = WindowConfig {
            width: 854,
            height: 480,
            fullscreen: false,
        };
        let (command, root) = command(LEGACY, Some(window));
        let main = command
            .args
            .iter()
            .position(|arg| arg == "net.minecraft.client.main.Main")
            .unwrap();
        assert_eq!(command.args[main - 2], "-cp");
        assert_eq!(
            command.args[4],
            format!("-Djava.library.path={}", p(&root.join("natives").join("1.8.9")))
        );
        assert_eq!(
            command.args[main + 1..],
            [
                "--username",
                "Steve",
                "--version",
                "1.8.9",
                "--gameDir",
                &p(&root.join("instances").join("default")),
                "--assetsDir",
                &p(&root.join("assets")),
                "--assetIndex",
                "1.8",
                "--uuid",
                "4566e69fc90748ee8d71d7ba5aa00d20",
                "--accessToken",
                "secret-token",
                "--userProperties",
                "{}",
                "--userType",
                "msa",
                "--width",
                "854",
                "--height",
                "480",
            ]
            .map(str::to_owned)
        );
    }

    #[test]
    fn redacts_the_session_token_for_logs() {
        let (command, _) = command(LEGACY, None);
        let redacted = command.redacted_args().join(" ");
        assert!(!redacted.contains("secret-token"));
        assert!(redacted.contains("--accessToken ********"));
    }

    #[test]
    fn substitution_keeps_unknown_and_unterminated_placeholders() {
        let values = HashMap::from([("a", "1".to_owned())]);
        assert_eq!(substitute("x${a}y${b}z", &values), "x1y${b}z");
        assert_eq!(substitute("${a", &values), "${a");
        assert_eq!(substitute("plain", &values), "plain");
    }
}
