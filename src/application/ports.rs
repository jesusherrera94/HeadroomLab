use std::path::{Path, PathBuf};

use crate::domain::doom::DoomControls;
use crate::domain::file_system::FileSystemError;
use crate::domain::plugin::PluginError;
use crate::domain::project::RecentProject;
use crate::domain::terminal::{ShellChoice, TerminalPalette, TerminalSize, TerminalSnapshot};
use crate::domain::update::{ReleaseInfo, UpdateError};
// Port: the application depends on this abstraction.

#[derive(Debug)]
pub struct RecentProjectsError(pub String);

impl std::fmt::Display for RecentProjectsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to persist recent projects: {}", self.0)
    }
}

impl std::error::Error for RecentProjectsError {}

pub trait RecentProjectsStore {
    fn load(&self) -> Vec<RecentProject>;
    fn save(&self, items: &[RecentProject]) -> Result<(), RecentProjectsError>;
}

#[derive(Debug)]
pub struct ProjectGenerationError(pub String);

impl std::fmt::Display for ProjectGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ProjectGenerationError {}


pub trait ProjectGeneratorPort {
    fn generate(&self, name: &str, path: &std::path::Path) -> Result<(), ProjectGenerationError>;
}


pub struct DirEntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub trait ProjectFileSystemPort {
    fn read_dir(&self, dir: &Path) -> Vec<DirEntryInfo>;

    fn exists(&self, path: &Path) -> bool;

    fn create_file(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError>;

    fn create_dir(&self, dir: &Path, name: &str) -> Result<PathBuf, FileSystemError>;

    fn rename(&self, path: &Path, new_name: &str) -> Result<PathBuf, FileSystemError>;

    fn delete_to_trash(&self, path: &Path) -> Result<(), FileSystemError>;

    fn reveal(&self, path: &Path) -> Result<(), FileSystemError>;

    fn read_file(&self, path: &Path) -> Result<Vec<u8>, FileSystemError>;

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError>;

    fn modified(&self, path: &Path) -> Option<std::time::SystemTime>;
}

pub trait FileWatchSession {
    fn drain(&self) -> Vec<PathBuf>;
}

pub trait FileWatcherPort {
    fn watch(&self, root: &Path) -> Result<Box<dyn FileWatchSession>, FileSystemError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalError(pub String);

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    Wakeup,
    Title(String),
    ChildExit(Option<i32>),
    ClipboardStore(String),
    ClipboardLoad,
    Bell,
}

pub trait TerminalPort {
    fn open(
        &self,
        shell: &ShellChoice,
        cwd: &Path,
        size: TerminalSize,
        palette: TerminalPalette,
    ) -> Result<Box<dyn TerminalSession>, TerminalError>;
}

pub trait TerminalSession {
    fn write(&self, bytes: &[u8]);

    fn resize(&self, size: TerminalSize);

    fn snapshot(&self) -> TerminalSnapshot;

    fn logical_text(&self) -> String;

    fn drain_events(&self) -> Vec<TerminalEvent>;

    fn scroll(&self, lines: i32);

    fn clear(&self);

    fn select(&self, range: Option<((u16, u16), (u16, u16))>);

    fn selection_text(&self) -> Option<String>;

    fn kill(&self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomError(pub String);

impl std::fmt::Display for DoomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub trait DoomPort {
    fn start(&self) -> Result<Box<dyn DoomGame>, DoomError>;
}

pub trait DoomGame {

    fn tick(&mut self, controls: DoomControls);

    fn frame(&self) -> &[u8];

    fn level_complete(&self) -> bool;

    fn player_dead(&self) -> bool;
}
pub trait ClipboardPort {
    fn read(&self) -> Option<String>;
}

pub struct AudioMetadata {
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub format: String,
}

pub struct AudioSnapshot {
    pub samples: std::sync::Arc<Vec<f32>>, // interleaved, original/unprocessed
    pub sample_rate: u32,
    pub channels: u16,
}

pub trait PluginLoaderPort {
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
    fn snapshot_samples(&self) -> Option<AudioSnapshot>;
}

pub trait UpdaterPort: Send + Sync {

    fn check(&self) -> Result<Option<ReleaseInfo>, UpdateError>;

    fn download_and_install(
        &self,
        release: &ReleaseInfo,
        on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(), UpdateError>;

    fn restart(&self) -> Result<std::convert::Infallible, UpdateError>;
}
