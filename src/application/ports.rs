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
    pub path: std::path::PathBuf,
    pub is_dir: bool,
}

/// Reads project directories on demand for the file-explorer tree. Injected into
/// the presentation layer so the UI never touches `std::fs` directly (keeping
/// the hexagonal layering intact). A future file-mutation task can extend this
/// port with create/rename/remove operations.
pub trait ProjectFileSystemPort {
    /// Reads the immediate children of `dir` (one level, not recursive).
    /// Entries are returned unsorted; ordering is the caller's concern. Any
    /// error (permissions, deleted mid-session) yields an empty list, so an
    /// unreadable folder renders as empty rather than crashing.
    fn read_dir(&self, dir: &std::path::Path) -> Vec<DirEntryInfo>;
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
