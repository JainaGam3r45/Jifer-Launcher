//! Filesystem helpers with the copy/move semantics the instance store relies on.

use std::fs;
use std::io;
use std::path::Path;

/// Renames `source` to `target`; across volumes falls back to copy-then-delete.
/// A failed copy removes its partial output so the source stays the single copy.
pub fn move_path(source: &Path, target: &Path) -> io::Result<()> {
    move_path_with(source, target, |from, to| fs::rename(from, to), copy_recursive)
}

fn move_path_with(
    source: &Path,
    target: &Path,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
    copy: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            if let Err(copy_error) = copy(source, target) {
                let _ = remove_any(target);
                return Err(copy_error);
            }
            remove_any(source)
        }
        Err(error) => Err(error),
    }
}

/// Copies a file or directory tree. Never overwrites: an existing destination file is an error.
pub fn copy_recursive(source: &Path, target: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &target.join(entry.file_name()))?;
        }
        Ok(())
    } else if metadata.is_file() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        fs::copy(source, target).map(|_| ())
    } else {
        // Symlinks and junctions inside game folders are skipped rather than followed out of the tree.
        Ok(())
    }
}

/// Like `copy_recursive`, but a missing source is not an error.
pub fn copy_if_exists(source: &Path, target: &Path) -> io::Result<()> {
    match copy_recursive(source, target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound && !source.exists() => Ok(()),
        other => other,
    }
}

pub fn remove_any(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(root: &Path) -> std::path::PathBuf {
        let source = root.join("saves");
        fs::create_dir_all(source.join("world")).unwrap();
        fs::write(source.join("world").join("level.dat"), "data").unwrap();
        source
    }

    fn cross_device(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::ErrorKind::CrossesDevices.into())
    }

    #[test]
    fn copy_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let source = seed(dir.path());
        let target = dir.path().join("copy");
        fs::create_dir_all(target.join("world")).unwrap();
        fs::write(target.join("world").join("level.dat"), "existing").unwrap();
        assert_eq!(
            copy_recursive(&source, &target).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            fs::read_to_string(target.join("world").join("level.dat")).unwrap(),
            "existing"
        );
    }

    #[test]
    fn renames_when_possible() {
        let dir = tempfile::tempdir().unwrap();
        let source = seed(dir.path());
        let target = dir.path().join("moved");
        move_path(&source, &target).unwrap();
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(target.join("world").join("level.dat")).unwrap(),
            "data"
        );
    }

    #[test]
    fn copies_then_removes_across_volumes() {
        let dir = tempfile::tempdir().unwrap();
        let source = seed(dir.path());
        let target = dir.path().join("moved");
        move_path_with(&source, &target, cross_device, copy_recursive).unwrap();
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(target.join("world").join("level.dat")).unwrap(),
            "data"
        );
    }

    #[test]
    fn keeps_the_source_when_the_rename_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let source = seed(dir.path());
        let target = dir.path().join("moved");
        let blocked = |_: &Path, _: &Path| Err(io::ErrorKind::PermissionDenied.into());
        let error = move_path_with(&source, &target, blocked, copy_recursive).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(source.join("world").join("level.dat").exists());
        assert!(!target.exists());
    }

    #[test]
    fn discards_a_partial_copy_and_keeps_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = seed(dir.path());
        let target = dir.path().join("moved");
        let failing_copy = |_: &Path, to: &Path| {
            fs::create_dir_all(to)?;
            fs::write(to.join("half.dat"), "partial")?;
            Err(io::Error::from(io::ErrorKind::StorageFull))
        };
        let error = move_path_with(&source, &target, cross_device, failing_copy).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
        assert!(source.join("world").join("level.dat").exists());
        assert!(!target.exists());
    }

    #[test]
    fn copy_if_exists_ignores_missing_sources() {
        let dir = tempfile::tempdir().unwrap();
        copy_if_exists(&dir.path().join("missing"), &dir.path().join("out")).unwrap();
        assert!(!dir.path().join("out").exists());
    }
}
