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

    pub fn create_file(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
        self.guard(dir, name)?;
        self.fs.create_file(dir, name.trim())
    }

    pub fn create_directory(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError> {
        self.guard(dir, name)?;
        self.fs.create_dir(dir, name.trim())
    }

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

    pub fn delete(&self, path: &Path) -> Result<(), FileSystemError> {
        self.fs.delete_to_trash(path)
    }

    pub fn reveal(&self, path: &Path) -> Result<(), FileSystemError> {
        self.fs.reveal(path)
    }
    pub fn open_document(&self, path: &Path) -> Result<OpenedDocument, FileSystemError> {
        let modified = self.fs.modified(path);
        let bytes = self.fs.read_file(path)?;
        Ok(OpenedDocument {
            content: classify(bytes),
            modified,
        })
    }

    pub fn save_document(
        &self,
        path: &Path,
        text: &str,
        crlf: bool,
    ) -> Result<Option<SystemTime>, FileSystemError> {
        self.fs.write_file(path, &to_disk_bytes(text, crlf))?;
        Ok(self.fs.modified(path))
    }

    fn guard(&self, dir: &Path, name: &str) -> Result<(), FileSystemError> {
        let name = name.trim();
        validate_entry_name(name)?;
        if self.fs.exists(&dir.join(name)) {
            return Err(FileSystemError::AlreadyExists(name.to_string()));
        }
        Ok(())
    }
}
