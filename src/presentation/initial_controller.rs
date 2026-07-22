//! Logic for the Initial window: owns the modal buffers and turns view events
//! into a navigation intent (the project to open in the Editor). Recording the
//! project in Recents and switching screens is the app controller's job.

use std::path::PathBuf;

use rfd::FileDialog;

use crate::domain::project::RecentProject;
use crate::presentation::windows::initial_window::InitialViewEvents;

/// Per-window state for the Initial screen.
#[derive(Default)]
pub struct InitialState {
    /// Whether the Create-project modal is showing.
    pub modal_open: bool,
    pub name: String,
    pub path: String,
    /// Set once the user edits Path manually, freezing the Name→Path sync.
    pub path_edited: bool,
    /// Set to bring the OS window to the front on the next frame.
    pub focus_requested: bool,
}

impl InitialState {
    /// Opens the Create modal with default buffers (path prefilled to
    /// `<Documents>/HeadroomLab/<name>`, which starts as the bare base dir).
    fn open_modal(&mut self) {
        self.name.clear();
        self.path_edited = false;
        self.path = default_path_for(&self.name).to_string_lossy().to_string();
        self.modal_open = true;
    }

    fn close_modal(&mut self) {
        self.modal_open = false;
    }

    /// Re-derives Path from Name while the user hasn't taken over the field.
    fn sync_path_to_name(&mut self) {
        if !self.path_edited {
            self.path = default_path_for(&self.name).to_string_lossy().to_string();
        }
    }
}

/// Handles a frame's worth of Initial-window events, returning the project to
/// open if the user chose one this frame.
pub fn handle_events(
    state: &mut InitialState,
    events: InitialViewEvents,
    recents: &[RecentProject],
) -> Option<RecentProject> {
    if events.create_clicked {
        state.open_modal();
    }

    if events.open_clicked
        && let Some(project) = pick_folder()
    {
        return Some(project);
    }

    if let Some(index) = events.recent_clicked
        && let Some(project) = recents.get(index)
    {
        return Some(project.clone());
    }

    if state.modal_open {
        if events.modal.name_changed {
            state.sync_path_to_name();
        }
        if events.modal.cancelled {
            state.close_modal();
        }
        if events.modal.created {
            let project = RecentProject::new(state.name.clone(), PathBuf::from(state.path.clone()));
            state.close_modal();
            return Some(project);
        }
    }

    None
}

/// Opens the native folder picker; on selection builds a project named after
/// the chosen folder (falling back to the full path if it has no file name).
fn pick_folder() -> Option<RecentProject> {
    let path = FileDialog::new().pick_folder()?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Some(RecentProject::new(name, path))
}

/// `<Documents>/HeadroomLab/<name>` (name may be empty). Falls back to the home
/// dir, then the current working dir, if no Documents dir is available.
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
