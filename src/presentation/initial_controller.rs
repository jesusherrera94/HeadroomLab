//! Logic for the Initial window: owns the modal buffers and turns view events
//! into a navigation intent (the project to open in the Editor). Recording the
//! project in Recents and switching screens is the app controller's job.

use std::path::{Path, PathBuf};

use rfd::FileDialog;

use crate::domain::project::{RecentProject, sanitize_target};
use crate::presentation::windows::initial_window::InitialViewEvents;

/// What the user asked to do this frame. The app controller acts on it:
/// `Create` generates the project first (and only records/navigates on
/// success); `Open` records + navigates directly (no generation).
pub enum ProjectIntent {
    Create(RecentProject),
    Open(RecentProject),
}

/// Per-window state for the Initial screen.
#[derive(Default)]
pub struct InitialState {
    /// Whether the Create-project modal is showing.
    pub modal_open: bool,
    pub name: String,
    pub path: String,
    /// Set once the user edits Path manually, freezing the Name→Path sync.
    pub path_edited: bool,
    /// Error from the last failed generation attempt, shown inside the modal.
    pub generation_error: Option<String>,
    /// Set to bring the OS window to the front on the next frame.
    pub focus_requested: bool,
}

impl InitialState {
    /// Opens the Create modal with default buffers (path prefilled to
    /// `<Documents>/HeadroomLab/<name>`, which starts as the bare base dir).
    fn open_modal(&mut self) {
        self.name.clear();
        self.path_edited = false;
        self.generation_error = None;
        self.path = default_path_for(&self.name).to_string_lossy().to_string();
        self.modal_open = true;
    }

    /// Opens the Create modal from outside the window — File ▸ New Project…,
    /// which lands on this screen with the modal already up.
    pub fn open_create_modal(&mut self) {
        self.open_modal();
    }

    /// Closes the Create modal (called by the app controller on Cancel or a
    /// successful generation).
    pub fn close_modal(&mut self) {
        self.modal_open = false;
        self.generation_error = None;
    }

    /// Re-derives Path from Name while the user hasn't taken over the field.
    fn sync_path_to_name(&mut self) {
        if !self.path_edited {
            self.path = default_path_for(&self.name).to_string_lossy().to_string();
        }
    }
}

/// Handles a frame's worth of Initial-window events, returning an intent when
/// the user chose an action this frame. For Create the modal is left open —
/// the app controller closes it only after generation succeeds (D10).
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
        // Editing either field clears a stale generation error and re-syncs
        // the path while the user hasn't taken it over.
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

/// Validates the Create inputs, returning an inline message (which disables
/// **Create**) or `None` when the project can be generated. Cheap enough to run
/// every frame: at worst one `read_dir` on the target path.
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
