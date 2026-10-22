// Transforms Mojang's json soup into verified download jobs and the directory layout
// that the JVM actually expects to find on disk.
//
// Layout under our shared Minecraft folder:
// `versions/<id>/<id>.jar`, `libraries/<maven path>`, `natives/<id>/`,
// and the eternal labyrinth of `assets/{indexes,objects,virtual}`.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use jifercraft_net::{Checksum, DownloadJob};

use crate::error::{CoreError, CoreResult, IoContext};
use crate::meta::parse_url;
use crate::version::{Artifact, AssetIndex, Environment, Library, VersionJson, rules_allow};

pub const ASSET_HOST: &str = "https://resources.download.minecraft.net";
const NATIVE_EXTENSIONS: [&str; 4] = ["dll", "so", "dylib", "jnilib"];

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub jobs: Vec<DownloadJob>,
    // Libraries go first, client jar goes last. Duplicates get tossed out.
    pub classpath: Vec<PathBuf>,
    pub native_archives: Vec<NativeArchive>,
    pub natives_dir: PathBuf,
    pub client_jar: PathBuf,
    // JVM flag with substituted path for Mojang's log4j xml layout config.
    pub logging_argument: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeArchive {
    pub path: PathBuf,
    pub exclude: Vec<String>,
}

pub fn plan_version(version: &VersionJson, root: &Path, env: &Environment) -> CoreResult<InstallPlan> {
    let libraries_dir = root.join("libraries");
    let mut plan = InstallPlan {
        jobs: Vec::new(),
        classpath: Vec::new(),
        native_archives: Vec::new(),
        natives_dir: root.join("natives").join(&version.id),
        client_jar: root
            .join("versions")
            .join(&version.id)
            .join(format!("{}.jar", version.id)),
        logging_argument: None,
    };
    let mut on_classpath = HashSet::new();

    for library in version
        .libraries
        .iter()
        .filter(|library| rules_allow(&library.rules, env))
    {
        if let Some(classifier) = native_classifier(library, env) {
            let artifact = library
                .downloads
                .as_ref()
                .and_then(|downloads| downloads.classifiers.as_ref())
                .and_then(|classifiers| classifiers.get(&classifier));
            if let Some(artifact) = artifact {
                let path = artifact_path(&libraries_dir, library, artifact, Some(&classifier))?;
                plan.jobs.push(job(artifact, path.clone())?);
                let exclude = library
                    .extract
                    .as_ref()
                    .map(|extract| extract.exclude.clone())
                    .unwrap_or_default();
                plan.native_archives.push(NativeArchive { path, exclude });
            }
        }

        let artifact = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref());
        let path = match artifact {
            Some(artifact) => {
                let path = artifact_path(&libraries_dir, library, artifact, None)?;
                plan.jobs.push(job(artifact, path.clone())?);
                path
            }
            None if library.natives.is_some() => continue,
            None => {
                let relative = maven_path(&library.name)?;
                let base = library.url.as_deref().unwrap_or("https://libraries.minecraft.net/");
                let url = parse_url("library URL", &format!("{}/{relative}", base.trim_end_matches('/')))?;
                let path = safe_join(&libraries_dir, &relative)?;
                plan.jobs.push(DownloadJob {
                    url,
                    dest: path.clone(),
                    size: None,
                    checksum: None,
                });
                path
            }
        };
        // Since 1.19 natives ship as regular classpath jars named `...:natives-<os>`.
        if library.name.contains(":natives-") {
            plan.native_archives.push(NativeArchive {
                path: path.clone(),
                exclude: vec!["META-INF/".to_owned()],
            });
        }
        if on_classpath.insert(path.clone()) {
            plan.classpath.push(path);
        }
    }

    let client = version
        .downloads
        .as_ref()
        .ok_or_else(|| metadata(&version.id, "the version has no client download"))?;
    plan.jobs.push(job(&client.client, plan.client_jar.clone())?);
    plan.classpath.push(plan.client_jar.clone());

    if let Some(logging) = version.logging.as_ref().and_then(|logging| logging.client.as_ref()) {
        let path = safe_join(&root.join("assets").join("log_configs"), &logging.file.id)?;
        plan.jobs.push(DownloadJob {
            url: parse_url("log config URL", &logging.file.url)?,
            dest: path.clone(),
            size: Some(logging.file.size),
            checksum: Some(Checksum::sha1_hex(&logging.file.sha1)?),
        });
        plan.logging_argument = Some(logging.argument.replace("${path}", &path.to_string_lossy()));
    }
    Ok(plan)
}

/// One job per asset object, stored content-addressed by SHA-1.
pub fn plan_assets(index: &AssetIndex, root: &Path) -> CoreResult<Vec<DownloadJob>> {
    let objects = root.join("assets").join("objects");
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for object in index.objects.values() {
        if !seen.insert(object.hash.as_str()) {
            continue;
        }
        let checksum = Checksum::sha1_hex(&object.hash)?;
        let prefix = &object.hash[..2];
        jobs.push(DownloadJob {
            url: parse_url("asset URL", &format!("{ASSET_HOST}/{prefix}/{}", object.hash))?,
            dest: objects.join(prefix).join(&object.hash),
            size: Some(object.size),
            checksum: Some(checksum),
        });
    }
    Ok(jobs)
}

// Minecraft archaeological expedition:
// In ancient versions (pre-1.7), Minecraft had no concept of content-addressed sha1 objects
// and expected sound files by raw filename in .minecraft/resources or virtual folders.
// If we don't reconstruct this virtual tree, running 1.5.2 gives you an eerie silent game.
pub fn legacy_assets_dir(root: &Path, assets_id: &str) -> PathBuf {
    root.join("assets").join("virtual").join(assets_id)
}

// Rebuilds the flat legacy asset files from our sha1-indexed object pool.
pub fn materialize_legacy_assets(index: &AssetIndex, root: &Path, assets_id: &str, game_dir: &Path) -> CoreResult<()> {
    let target_base = if index.map_to_resources {
        game_dir.join("resources")
    } else if index.is_virtual {
        legacy_assets_dir(root, assets_id)
    } else {
        return Ok(());
    };
    let objects = root.join("assets").join("objects");
    for (name, object) in &index.objects {
        Checksum::sha1_hex(&object.hash)?;
        let target = safe_join(&target_base, name)?;
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).at(parent)?;
        }
        let source = objects.join(&object.hash[..2]).join(&object.hash);
        fs::copy(&source, &target).at(&source)?;
    }
    Ok(())
}

/// Extracts native libraries flat into `natives_dir`, skipping excluded prefixes and anything that is not a
/// shared library. Entry names are only used for their file name, so archive paths cannot escape. Blocking.
pub fn extract_natives(archives: &[NativeArchive], natives_dir: &Path) -> CoreResult<()> {
    fs::create_dir_all(natives_dir).at(natives_dir)?;
    for archive in archives {
        let file = fs::File::open(&archive.path).at(&archive.path)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|error| zip_error(&archive.path, &error))?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|error| zip_error(&archive.path, &error))?;
            let name = entry
                .name()
                .map_err(|error| zip_error(&archive.path, &error))?
                .into_owned();
            if entry.is_dir() || archive.exclude.iter().any(|prefix| name.starts_with(prefix.as_str())) {
                continue;
            }
            let Some(file_name) = Path::new(&name).file_name().map(Path::new) else {
                continue;
            };
            let is_native = file_name
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| NATIVE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
            if !is_native {
                continue;
            }
            let target = natives_dir.join(file_name);
            // A running game keeps its DLLs locked; identical files are left alone instead of rewritten.
            if fs::metadata(&target).is_ok_and(|meta| meta.len() == entry.size()) {
                continue;
            }
            let mut output = fs::File::create(&target).at(&target)?;
            io::copy(&mut entry, &mut output).at(&target)?;
        }
    }
    Ok(())
}

fn native_classifier(library: &Library, env: &Environment) -> Option<String> {
    let natives = library.natives.as_ref()?;
    natives
        .get(env.os)
        .map(|classifier| classifier.replace("${arch}", env.arch_bits()))
}

fn artifact_path(base: &Path, library: &Library, artifact: &Artifact, classifier: Option<&str>) -> CoreResult<PathBuf> {
    let relative = match (&artifact.path, classifier) {
        (Some(path), _) => path.clone(),
        (None, None) => maven_path(&library.name)?,
        (None, Some(classifier)) => maven_path(&format!("{}:{classifier}", library.name))?,
    };
    safe_join(base, &relative)
}

fn job(artifact: &Artifact, dest: PathBuf) -> CoreResult<DownloadJob> {
    Ok(DownloadJob {
        url: parse_url("download URL", &artifact.url)?,
        dest,
        size: Some(artifact.size),
        checksum: Some(Checksum::sha1_hex(&artifact.sha1)?),
    })
}

/// `group:artifact:version[:classifier][@ext]` to `group/path/artifact/version/artifact-version[-classifier].ext`.
pub fn maven_path(coordinate: &str) -> CoreResult<String> {
    let (coordinate, extension) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    let parts: Vec<&str> = coordinate.split(':').collect();
    let (group, artifact, version, classifier) = match parts.as_slice() {
        [group, artifact, version] => (group, artifact, version, None),
        [group, artifact, version, classifier] => (group, artifact, version, Some(classifier)),
        _ => return Err(metadata(coordinate, "not a maven coordinate")),
    };
    let file = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    Ok(format!("{}/{artifact}/{version}/{file}", group.replace('.', "/")))
}

/// Joins a relative path taken from metadata, refusing anything that could leave `base`.
pub fn safe_join(base: &Path, relative: &str) -> CoreResult<PathBuf> {
    let candidate = Path::new(relative);
    let normal = candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if relative.is_empty() || !normal || relative.contains('\\') && cfg!(not(windows)) {
        return Err(metadata(relative, "unsafe relative path"));
    }
    Ok(base.join(candidate))
}

fn metadata(what: &str, reason: &str) -> CoreError {
    CoreError::Metadata {
        what: what.to_owned(),
        reason: reason.to_owned(),
    }
}

fn zip_error(path: &Path, error: &zip::result::ZipError) -> CoreError {
    metadata(&path.display().to_string(), &error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const WINDOWS: Environment = Environment {
        os: "windows",
        arch: "x86_64",
        custom_resolution: false,
    };
    const SHA: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

    fn version() -> VersionJson {
        serde_json::from_str(&format!(
            r#"{{
            "id": "1.20.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "downloads": {{ "client": {{ "sha1": "{SHA}", "size": 10, "url": "https://piston-data.test/client.jar" }} }},
            "logging": {{ "client": {{ "argument": "-Dlog4j.configurationFile=${{path}}",
                "file": {{ "id": "client-1.12.xml", "sha1": "{SHA}", "size": 5, "url": "https://piston-data.test/log.xml" }} }} }},
            "libraries": [
              {{ "name": "com.mojang:brigadier:1.1.8",
                 "downloads": {{ "artifact": {{ "path": "com/mojang/brigadier/1.1.8/brigadier-1.1.8.jar", "sha1": "{SHA}", "size": 1, "url": "https://libraries.test/b.jar" }} }} }},
              {{ "name": "org.lwjgl:lwjgl:3.3.1:natives-windows",
                 "downloads": {{ "artifact": {{ "path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar", "sha1": "{SHA}", "size": 2, "url": "https://libraries.test/n.jar" }} }},
                 "rules": [{{ "action": "allow", "os": {{ "name": "windows" }} }}] }},
              {{ "name": "org.lwjgl:lwjgl:3.3.1:natives-macos",
                 "downloads": {{ "artifact": {{ "path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar", "sha1": "{SHA}", "size": 2, "url": "https://libraries.test/m.jar" }} }},
                 "rules": [{{ "action": "allow", "os": {{ "name": "osx" }} }}] }},
              {{ "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
                 "natives": {{ "windows": "natives-windows-${{arch}}" }}, "extract": {{ "exclude": ["META-INF/"] }},
                 "downloads": {{ "classifiers": {{ "natives-windows-64": {{ "path": "org/lwjgl/lwjgl-platform/2.9.4/p-64.jar", "sha1": "{SHA}", "size": 3, "url": "https://libraries.test/p.jar" }} }} }} }},
              {{ "name": "com.mojang:brigadier:1.1.8",
                 "downloads": {{ "artifact": {{ "path": "com/mojang/brigadier/1.1.8/brigadier-1.1.8.jar", "sha1": "{SHA}", "size": 1, "url": "https://libraries.test/b.jar" }} }} }}
            ] }}"#
        ))
        .unwrap()
    }

    #[test]
    fn plans_libraries_natives_client_and_logging() {
        let root = Path::new("/mc");
        let plan = plan_version(&version(), root, &WINDOWS).unwrap();
        let libraries = root.join("libraries");
        assert_eq!(
            plan.classpath,
            vec![
                libraries.join("com/mojang/brigadier/1.1.8/brigadier-1.1.8.jar"),
                libraries.join("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"),
                root.join("versions/1.20.1/1.20.1.jar"),
            ]
        );
        assert_eq!(plan.native_archives.len(), 2);
        assert_eq!(
            plan.native_archives[1].path,
            libraries.join("org/lwjgl/lwjgl-platform/2.9.4/p-64.jar")
        );
        assert!(plan.jobs.iter().all(|job| job.checksum.is_some() && job.size.is_some()));
        assert!(
            !plan.jobs.iter().any(|job| job.url.as_str().ends_with("m.jar")),
            "macOS natives are filtered out"
        );
        let log = root.join("assets").join("log_configs").join("client-1.12.xml");
        assert_eq!(
            plan.logging_argument,
            Some(format!("-Dlog4j.configurationFile={}", log.display()))
        );
        assert_eq!(plan.natives_dir, root.join("natives/1.20.1"));
    }

    #[test]
    fn plans_content_addressed_assets_once_per_hash() {
        let index: AssetIndex = serde_json::from_str(&format!(
            r#"{{"objects":{{"a.png":{{"hash":"{SHA}","size":5}},"b.png":{{"hash":"{SHA}","size":5}}}}}}"#
        ))
        .unwrap();
        let jobs = plan_assets(&index, Path::new("/mc")).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].url.as_str(), format!("{ASSET_HOST}/aa/{SHA}"));
        assert_eq!(jobs[0].dest, Path::new("/mc/assets/objects/aa").join(SHA));
    }

    #[test]
    fn rejects_hostile_paths_and_hashes() {
        for path in ["../evil.jar", "/etc/passwd", "a/../../b", ""] {
            assert!(safe_join(Path::new("/mc/libraries"), path).is_err(), "{path}");
        }
        let index: AssetIndex =
            serde_json::from_str(r#"{"objects":{"x":{"hash":"../../../../windows","size":1}}}"#).unwrap();
        assert!(plan_assets(&index, Path::new("/mc")).is_err());
    }

    #[test]
    fn builds_maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.15.0").unwrap(),
            "net/fabricmc/fabric-loader/0.15.0/fabric-loader-0.15.0.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.1:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20.1@zip").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1.zip"
        );
        assert!(maven_path("broken").is_err());
    }

    #[test]
    fn extracts_only_shared_libraries_flat_and_skips_excluded_entries() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("natives.jar");
        let mut writer = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("windows/x64/org/lwjgl/lwjgl.dll", "dll"),
            ("../../escape.dll", "evil"),
            ("META-INF/signed.dll", "excluded"),
            ("org/lwjgl/Thing.class", "class"),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        writer.finish().unwrap();

        let natives = dir.path().join("natives");
        let archive = NativeArchive {
            path: jar,
            exclude: vec!["META-INF/".into()],
        };
        extract_natives(std::slice::from_ref(&archive), &natives).unwrap();
        extract_natives(&[archive], &natives).unwrap();

        let mut names: Vec<_> = fs::read_dir(&natives)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["escape.dll", "lwjgl.dll"]);
        assert!(!dir.path().join("escape.dll").exists());
    }

    #[test]
    fn materializes_virtual_assets_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let object = root.join("assets/objects/aa").join(SHA);
        fs::create_dir_all(object.parent().unwrap()).unwrap();
        fs::write(&object, "hello").unwrap();
        let index: AssetIndex = serde_json::from_str(&format!(
            r#"{{"virtual":true,"objects":{{"sound/step.ogg":{{"hash":"{SHA}","size":5}}}}}}"#
        ))
        .unwrap();
        materialize_legacy_assets(&index, root, "legacy", &root.join("game")).unwrap();
        assert_eq!(
            fs::read_to_string(legacy_assets_dir(root, "legacy").join("sound/step.ogg")).unwrap(),
            "hello"
        );
    }
}
