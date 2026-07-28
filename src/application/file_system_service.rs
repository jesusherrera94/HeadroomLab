//! Reusable file-system actions for the explorer (and, later, the App menu).
//!
//! This is the single, UI-agnostic surface for creating, renaming, deleting and
//! revealing project entries. Name validation and duplicate checks happen here
//! (via the domain policy) so every caller — the explorer today, the App menu
//! tomorrow — gets identical rules and errors. The actual disk work is delegated
//! to `ProjectFileSystemPort`.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use crate::application::ports::ProjectFileSystemPort;
use crate::domain::file_system::{FileSystemError, validate_entry_name};
use crate::domain::text_document::{DocumentContent, classify, to_disk_bytes};

/// A file read for editing: its classified content plus the modification time it
/// was read at, so the caller can tell a later external change from its own save.
pub struct OpenedDocument {
    pub content: DocumentContent,
    pub modified: Option<SystemTime>,
}

pub struct FileSystemService {
    fs: Rc<dyn ProjectFileSystemPort>,
}

impl FileSystemService {
    pub fn new(fs: Rc<dyn ProjectFileSystemPort>) -> Self {
        Self { fs }
    }

    /// Creates an empty file `name` inside `dir`.
    pub fn create_file(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
        self.guard(dir, name)?;
        self.fs.create_file(dir, name.trim())
    }

    /// Creates a directory `name` inside `dir`.
    pub fn create_directory(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
        self.guard(dir, name)?;
        self.fs.create_dir(dir, name.trim())
    }

    /// Renames the entry at `path` to `new_name` (same parent). A no-op rename to
    /// the current name returns the path unchanged.
    pub fn rename(&self, path: &Path, new_name: &str) -> Result<PathBuf, FileSystemError> {
        let new_name = new_name.trim();
        if path.file_name().and_then(|n| n.to_str()) == Some(new_name) {
            return Ok(path.to_path_buf());
        }
        validate_entry_name(new_name)?;
        let parent = path.parent().unwrap_or(Path::new(""));
        if self.fs.exists(&parent.join(new_name)) {
            return Err(FileSystemError::AlreadyExists(new_name.to_string()));
        }
        self.fs.rename(path, new_name)
    }

    /// Moves `path` (file or directory, recursively) to the OS trash.
    pub fn delete(&self, path: &Path) -> Result<(), FileSystemError> {
        self.fs.delete_to_trash(path)
    }

    /// Reveals `path` in the platform file manager.
    pub fn reveal(&self, path: &Path) -> Result<(), FileSystemError> {
        self.fs.reveal(path)
    }

    /// Reads a file for the editor and applies the open-time policy (size cap,
    /// UTF-8 validity, highlight threshold). The mtime is captured *before* the
    /// read so a change racing the read is detected on the next tick rather than
    /// being silently baked in as current.
    pub fn open_document(&self, path: &Path) -> Result<OpenedDocument, FileSystemError> {
        let modified = self.fs.modified(path);
        let bytes = self.fs.read_file(path)?;
        Ok(OpenedDocument {
            content: classify(bytes),
            modified,
        })
    }

    /// Writes `text` back to `path`, restoring CRLF line endings if the file had
    /// them. Returns the post-write mtime so the caller can store it and ignore
    /// the watcher event this write is about to produce.
    pub fn save_document(
        &self,
        path: &Path,
        text: &str,
        crlf: bool,
    ) -> Result<Option<SystemTime>, FileSystemError> {
        self.fs.write_file(path, &to_disk_bytes(text, crlf))?;
        Ok(self.fs.modified(path))
    }

    /// Shared create precondition: valid name and no existing entry.
    fn guard(&self, dir: &Path, name: &str) -> Result<(), FileSystemError> {
        let name = name.trim();
        validate_entry_name(name)?;
        if self.fs.exists(&dir.join(name)) {
            return Err(FileSystemError::AlreadyExists(name.to_string()));
        }
        Ok(())
    }
}
