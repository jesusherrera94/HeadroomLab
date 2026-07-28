//! `std::fs`-backed adapter for `ProjectFileSystemPort`. Reads directories one
//! level at a time (lazy, per HL10) and performs the explorer's mutations:
//! create, rename, delete-to-trash (`trash` crate), and reveal (`opener`).

use std::path::{Path, PathBuf};

use crate::application::ports::{DirEntryInfo, ProjectFileSystemPort};
use crate::domain::file_system::{FileSystemError, validate_entry_name};

pub struct StdFsProjectFileSystem;

impl StdFsProjectFileSystem {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdFsProjectFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates `name` and returns the destination path inside `dir`, erroring if
/// something already exists there.
fn destination(dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
    validate_entry_name(name)?;
    let target = dir.join(name.trim());
    if target.exists() {
        return Err(FileSystemError::AlreadyExists(name.trim().to_string()));
    }
    Ok(target)
}

impl ProjectFileSystemPort for StdFsProjectFileSystem {
    fn read_dir(&self, dir: &Path) -> Vec<DirEntryInfo> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| DirEntryInfo {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                // `file_type` avoids following symlinks and an extra `stat`; a
                // symlink is rendered by its own kind, which is fine for display.
                is_dir: entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
            })
            .collect()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_file(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
        let target = destination(dir, name)?;
        std::fs::File::create(&target).map_err(|e| FileSystemError::Io(e.to_string()))?;
        Ok(target)
    }

    fn create_dir(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
        let target = destination(dir, name)?;
        std::fs::create_dir(&target).map_err(|e| FileSystemError::Io(e.to_string()))?;
        Ok(target)
    }

    fn rename(&self, path: &Path, new_name: &str) -> Result<PathBuf, FileSystemError> {
        if !path.exists() {
            return Err(FileSystemError::NotFound(path.display().to_string()));
        }
        let parent = path.parent().unwrap_or(Path::new(""));
        let target = destination(parent, new_name)?;
        std::fs::rename(path, &target).map_err(|e| FileSystemError::Io(e.to_string()))?;
        Ok(target)
    }

    fn delete_to_trash(&self, path: &Path) -> Result<(), FileSystemError> {
        if !path.exists() {
            return Err(FileSystemError::NotFound(path.display().to_string()));
        }
        trash::delete(path).map_err(|e| FileSystemError::Io(e.to_string()))
    }

    fn reveal(&self, path: &Path) -> Result<(), FileSystemError> {
        opener::reveal(path).map_err(|e| FileSystemError::Io(e.to_string()))
    }

    fn read_file(&self, path: &Path) -> Result<Vec<u8>, FileSystemError> {
        std::fs::read(path).map_err(|e| io_error(path, e))
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError> {
        // No existence check: writing recreates a file deleted from under a
        // dirty buffer, which is exactly the recovery path we want.
        std::fs::write(path, contents).map_err(|e| io_error(path, e))
    }

    fn modified(&self, path: &Path) -> Option<std::time::SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }
}

/// Maps an IO failure onto the domain error, keeping "gone" distinct from the
/// rest so the UI can say something specific.
fn io_error(path: &Path, e: std::io::Error) -> FileSystemError {
    match e.kind() {
        std::io::ErrorKind::NotFound => FileSystemError::NotFound(path.display().to_string()),
        _ => FileSystemError::Io(e.to_string()),
    }
}
