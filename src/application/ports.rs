use std::path::{Path, PathBuf};

use crate::domain::file_system::FileSystemError;
use crate::domain::plugin::PluginError;
use crate::domain::project::RecentProject;
// Port: the application depends on this abstraction.

/// Failure persisting the recents list. Save failures are logged, never
/// surfaced to the UI, so a single opaque variant is enough.
#[derive(Debug)]
pub struct RecentProjectsError(pub String);

impl std::fmt::Display for RecentProjectsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to persist recent projects: {}", self.0)
    }
}

impl std::error::Error for RecentProjectsError {}

/// Persistence boundary for the recent-projects list. A read error yields an
/// empty list (never blocks startup); a write error is returned so the service
/// can log it.
pub trait RecentProjectsStore {
    fn load(&self) -> Vec<RecentProject>;
    fn save(&self, items: &[RecentProject]) -> Result<(), RecentProjectsError>;
}

/// Failure generating a new project on disk. The message is shown inline in the
/// Create modal; on failure nothing is recorded in Recents.
#[derive(Debug)]
pub struct ProjectGenerationError(pub String);

impl std::fmt::Display for ProjectGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ProjectGenerationError {}

/// Writes a new, compilable passthrough effect project to disk. The UI-facing
/// boundary for the Create flow (like `AudioEnginePort`), so `HeadroomApp`
/// holds it directly rather than through a service.
pub trait ProjectGeneratorPort {
    /// Creates `path` (which must be missing or an empty directory) and writes
    /// the template project into it. `name` is the display name; the build
    /// target / main-file name is derived from it via `sanitize_target`.
    fn generate(&self, name: &str, path: &std::path::Path) -> Result<(), ProjectGenerationError>;
}

/// One entry read from a project directory (a single, non-recursive level).
pub struct DirEntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Reads and mutates project files/directories for the explorer. Injected into
/// the presentation layer so the UI never touches `std::fs` directly (keeping
/// the hexagonal layering intact). Mutations return real errors; `read_dir`
/// stays lenient (empty on failure) so the tree renders regardless.
pub trait ProjectFileSystemPort {
    /// Reads the immediate children of `dir` (one level, not recursive).
    /// Entries are returned unsorted; ordering is the caller's concern. Any
    /// error (permissions, deleted mid-session) yields an empty list, so an
    /// unreadable folder renders as empty rather than crashing.
    fn read_dir(&self, dir: &Path) -> Vec<DirEntryInfo>;

    /// Whether `path` currently exists (used for duplicate/existence checks).
    fn exists(&self, path: &Path) -> bool;

    /// Creates an empty file `name` inside `dir`; returns the new path.
    fn create_file(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError>;

    /// Creates a directory `name` inside `dir`; returns the new path.
    fn create_dir(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError>;

    /// Renames the entry at `path` to `new_name` (kept in the same parent);
    /// returns the new path.
    fn rename(&self, path: &Path, new_name: &str) -> Result<PathBuf, FileSystemError>;

    /// Moves `path` (file or directory, recursively) to the OS trash.
    fn delete_to_trash(&self, path: &Path) -> Result<(), FileSystemError>;

    /// Reveals `path` in the platform file manager (Finder / Explorer).
    fn reveal(&self, path: &Path) -> Result<(), FileSystemError>;

    /// Reads the whole file as raw bytes. Deciding whether those bytes are
    /// editable text, a binary, or simply too big is the domain's job
    /// (`domain::text_document::classify`), not the adapter's.
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, FileSystemError>;

    /// Overwrites `path` with `contents`, creating the file if it has since been
    /// deleted (a dirty buffer must always be able to save itself back).
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError>;

    /// Last-modified time, or `None` when unavailable. Used to spot external
    /// edits and to recognise (and ignore) the watcher event our own save fires.
    fn modified(&self, path: &Path) -> Option<std::time::SystemTime>;
}

/// A running recursive filesystem watch. `drain` is non-blocking and returns the
/// paths that changed since the last call; the concrete watcher (e.g. `notify`)
/// is hidden behind this trait so no external types leak through the port.
pub trait FileWatchSession {
    fn drain(&self) -> Vec<PathBuf>;
}

/// Starts watching a project directory tree for external changes.
pub trait FileWatcherPort {
    fn watch(&self, root: &Path) -> Result<Box<dyn FileWatchSession>, FileSystemError>;
}

/// Reads the system clipboard, for the code editor's **Paste** menu item.
///
/// Read-only by design. egui exposes no clipboard read at all — it only receives
/// paste events the OS sends it, which is why the keyboard `⌘V` needs nothing
/// from us — but it *does* own clipboard writes through `Context::copy_text`,
/// and those must keep going through egui: on X11 the copying process has to
/// stay alive to serve the selection, so a second owner in the same app would
/// fight it.
///
/// `None` covers both an empty clipboard and one that cannot be reached at all
/// (no display server, another process holding it). The caller greys out Paste
/// either way — a clipboard that isn't there is not something the user can act
/// on, so it never reaches the error banner.
pub trait ClipboardPort {
    fn read(&self) -> Option<String>;
}

pub struct AudioMetadata {
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub format: String,
}

/// A point-in-time view of the original (unprocessed) decoded audio buffer,
/// used to render the graph view without touching the live playback state.
/// Shares the buffer via `Arc` so taking a snapshot is O(1) and never stalls
/// the audio callback waiting on the playback mutex.
pub struct AudioSnapshot {
    pub samples: std::sync::Arc<Vec<f32>>, // interleaved, original/unprocessed
    pub sample_rate: u32,
    pub channels: u16,
}

pub trait PluginLoaderPort {
    /// Load the effect from `path`. Replaces any previously loaded plugin.
    fn load_plugin(&self, path: &str) -> Result<(), PluginError>;
    fn unload_plugin(&self);
}

pub trait AudioEnginePort {
    fn load_file(&self, path: &str) -> Result<AudioMetadata, String>;
    fn play(&self);
    fn stop(&self);
    fn set_bypass(&self, enabled: bool);
    fn seek(&self, time_seconds: f32);
    fn add_processor(&self, processor: Box<dyn crate::domain::audio_processor::AudioProcessor>);
    fn clear_processors(&self);
    fn current_position(&self) -> f32;
    fn is_playing(&self) -> bool;
    /// Returns a copy of the original decoded buffer, or `None` if no audio is loaded.
    fn snapshot_samples(&self) -> Option<AudioSnapshot>;
}
