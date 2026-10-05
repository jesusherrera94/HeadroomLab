use std::path::{Path, PathBuf};

use rfd::FileDialog;

use crate::domain::project::{RecentProject, sanitize_target};
use crate::presentation::windows::initial_window::InitialViewEvents;

pub enum ProjectIntent {
    Create(RecentProject),
    Open(RecentProject),
}

#[derive(Default)]
pub struct InitialState {
    pub modal_open: bool,
    pub name: String,
    pub path: String,
    pub path_edited: bool,
    pub generation_error: Option<String>,
    pub focus_requested: bool,
}

impl InitialState {
    fn open_modal(&mut self) {
        self.name.clear();
        self.path_edited = false;
        self.generation_error = None;
        self.path = default_path_for(&self.name).to_string_lossy().to_string();
        self.modal_open = true;
    }

    pub fn open_create_modal(&mut self) {
        self.open_modal();
    }

    pub fn close_modal(&mut self) {
        self.modal_open = false;
        self.generation_error = None;
    }

    fn sync_path_to_name(&mut self) {
        if !self.path_edited {
            self.path = default_path_for(&self.name).to_string_lossy().to_string();
        }
    }
}

pub fn handle_events(
    state: &mut InitialState,
    events: InitialViewEvents,
    recents: &[RecentProject],
) -> Option<ProjectIntent> {
    if events.create_clicked {
        state.open_modal();
    }

    if events.open_clicked
        && let Some(project) = pick_folder()
    {
        return Some(ProjectIntent::Open(project));
    }

    if let Some(index) = events.recent_clicked
        && let Some(project) = recents.get(index)
    {
        return Some(ProjectIntent::Open(project.clone()));
    }

    if state.modal_open {
        if events.modal.name_changed {
            state.sync_path_to_name();
            state.generation_error = None;
        }
        if events.modal.path_changed {
            state.generation_error = None;
        }
        if events.modal.cancelled {
            state.close_modal();
        }
        if events.modal.created {
            let project = RecentProject::new(state.name.clone(), PathBuf::from(state.path.clone()));
            return Some(ProjectIntent::Create(project));
        }
    }

    None
}

pub fn create_validation(name: &str, path: &str) -> Option<String> {
    if sanitize_target(name).is_none() {
        return Some("Enter a name with at least one letter or digit.".to_string());
    }
    if path.trim().is_empty() {
        return Some("Enter a save path.".to_string());
    }

    let dir = Path::new(path.trim());
    if dir.is_file() {
        return Some("That path is a file, not a folder.".to_string());
    }
    if dir.is_dir() && dir_non_empty(dir) {
        return Some("That folder already exists and isn't empty.".to_string());
    }

    None
}

fn dir_non_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn pick_folder() -> Option<RecentProject> {
    let path = FileDialog::new().pick_folder()?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Some(RecentProject::new(name, path))
}

pub fn default_path_for(name: &str) -> PathBuf {
    project_base().join("HeadroomLab").join(name)
}

fn project_base() -> PathBuf {
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(docs) = dirs.document_dir() {
            return docs.to_path_buf();
        }
        return dirs.home_dir().to_path_buf();
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
