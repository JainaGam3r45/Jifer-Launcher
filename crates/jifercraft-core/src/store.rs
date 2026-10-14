//! Crash-safe JSON persistence: write a sibling temp file, flush it to disk, then rename over the target.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{CoreError, CoreResult, IoContext};

/// Pretty JSON with a trailing newline, matching what the Electron build wrote.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let mut body = serde_json::to_vec_pretty(value).map_err(|source| CoreError::Json {
        path: path.into(),
        source,
    })?;
    body.push(b'\n');
    write_atomic(path, &body)
}

pub fn write_atomic(path: &Path, body: &[u8]) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).at(parent)?;
    }
    let temp = temp_path(path);
    let result = write_and_rename(&temp, path, body);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_and_rename(temp: &Path, target: &Path, body: &[u8]) -> CoreResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(temp)
        .at(temp)?;
    file.write_all(body).at(temp)?;
    file.sync_all().at(temp)?;
    drop(file);
    // std's rename replaces an existing target on Windows (MoveFileExW with REPLACE_EXISTING).
    rename_with_retry(temp, target).at(target)
}

/// Windows Defender y los demonios de búsqueda adoran inspeccionar archivos temporales recién
/// cerrados justo en la milésima de segundo en que intentamos renombrarlos.
/// Si recibimos `ERROR_SHARING_VIOLATION` (32) o `ERROR_ACCESS_DENIED` (5), respiramos hondo,
/// esperamos un instante con backoff en lugar de tirar la toalla y corromper el guardado del usuario.
fn rename_with_retry(temp: &Path, target: &Path) -> std::io::Result<()> {
    let mut attempts = 0;
    loop {
        match fs::rename(temp, target) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let is_transient = is_transient_lock(&error);
                if is_transient && attempts < 5 {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20 * (1 << attempts)));
                    continue;
                }
                return Err(error);
            }
        }
    }
}

fn is_transient_lock(error: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        // 32: ERROR_SHARING_VIOLATION
        // 5:  ERROR_ACCESS_DENIED
        // 33: ERROR_LOCK_VIOLATION
        // 1224: ERROR_USER_MAPPED_FILE (Windows Defender mapea en memoria con CreateFileMapping el archivo recién cerrado)
        matches!(error.raw_os_error(), Some(32 | 5 | 33 | 1224))
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}

/// `Ok(None)` when the file does not exist.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> CoreResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| CoreError::Json {
                path: path.into(),
                source,
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::io(path, error)),
    }
}

/// Removes a temp file left behind by a crash between write and rename.
pub fn discard_stale_temp(path: &Path) {
    let _ = fs::remove_file(temp_path(path));
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Doc {
        value: u32,
    }

    #[test]
    fn round_trips_and_overwrites_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("doc.json");
        write_json_atomic(&path, &Doc { value: 1 }).unwrap();
        write_json_atomic(&path, &Doc { value: 2 }).unwrap();
        assert_eq!(read_json::<Doc>(&path).unwrap(), Some(Doc { value: 2 }));
        assert!(fs::read_to_string(&path).unwrap().ends_with("}\n"));
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("doc.json")]);
    }

    #[test]
    fn missing_file_is_none_and_garbage_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        assert_eq!(read_json::<Doc>(&path).unwrap(), None);
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(read_json::<Doc>(&path).unwrap_err().code(), "invalid_json");
    }

    #[test]
    fn discards_stale_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        fs::write(dir.path().join("doc.json.tmp"), "partial").unwrap();
        discard_stale_temp(&path);
        assert!(!dir.path().join("doc.json.tmp").exists());
    }
}
