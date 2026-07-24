//! `std::fs`-backed adapter for `ProjectFileSystemPort`. A thin, one-level
//! directory reader used by the editor's file-explorer tree. Reads are lazy
//! (one call per folder expansion), so heavy directories like `target/` and
//! `.git/` cost nothing until the user opens them.

use std::path::Path;

use crate::application::ports::{DirEntryInfo, ProjectFileSystemPort};

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
}
