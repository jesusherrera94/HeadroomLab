mod confirm;
mod events;
mod explorer;
mod file_tree;
mod tabs;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::application::file_system_service::FileSystemService;
use crate::application::ports::{
    ClipboardPort, FileWatchSession, FileWatcherPort, ProjectFileSystemPort, TerminalPort,
};
use crate::domain::editing::EditorCommand;
use crate::domain::project::RecentProject;
use crate::presentation::terminal_controller::TerminalState;

pub use confirm::{
    ConfirmChoice, ConfirmOutcome, EditorAction, PendingConfirm, request_quit,
    request_switch_project,
};
pub use events::{CodeEvents, EditorRequests, EditorViewEvents, ExplorerEvents, handle_events};
pub use explorer::{
    EntryKind, ExplorerUiState, PendingCreate, PendingRename, begin_create_at_selection,
    entry_kind_icon,
};
pub use file_tree::{
    FileTreeState, NodeIcon, TreeNode, create_target, icon_for_file, natural_cmp, row_icon,
};
pub use tabs::{
    EditorTab, TabId, close_active_tab, has_unsaved_work, jump_to_diagnostic, move_tab, open_find,
    save_active_tab, save_all_tabs, take_discarded_tabs, unsaved_paths,
};

use file_tree::{find_dir_mut, refresh_dir};
use tabs::{prune_missing, sync_open_buffers};

pub fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Show in Files"
    }
}

pub fn save_shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘S"
    } else {
        "Ctrl+S"
    }
}

pub struct EditorState {
    pub project_name: String,
    pub project_path: PathBuf,
    pub fs: Rc<dyn ProjectFileSystemPort>,
    pub fs_service: Rc<FileSystemService>,
    pub clipboard: Rc<dyn ClipboardPort>,
    pub watch: Option<Box<dyn FileWatchSession>>,
    pub tree: FileTreeState,
    pub explorer: ExplorerUiState,
    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,
    next_tab_id: u64,
    discarded_tabs: Vec<TabId>,
    pub scroll_active_into_view: bool,
    pub cursor: Option<(usize, usize)>,
    pub terminal: TerminalState,
    pub focus_requested: bool,
    pending_commands: VecDeque<EditorCommand>,
}

impl EditorState {
    pub fn new(
        project: &RecentProject,
        fs: Rc<dyn ProjectFileSystemPort>,
        fs_service: Rc<FileSystemService>,
        watcher: Rc<dyn FileWatcherPort>,
        clipboard: Rc<dyn ClipboardPort>,
        terminal: Rc<dyn TerminalPort>,
    ) -> Self {
        let tree = FileTreeState::new(project, fs.as_ref());

        let watch = match watcher.watch(&project.path) {
            Ok(session) => Some(session),
            Err(e) => {
                eprintln!("file watcher unavailable, external changes won't sync: {e}");
                None
            }
        };

        Self {
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            fs,
            fs_service,
            clipboard: clipboard.clone(),
            watch,
            tree,
            explorer: ExplorerUiState::default(),
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            discarded_tabs: Vec::new(),
            scroll_active_into_view: false,
            cursor: None,
            terminal: TerminalState::new(
                terminal,
                clipboard,
                crate::presentation::theme::terminal_palette(),
                &project.path,
            ),
            focus_requested: false,
            pending_commands: VecDeque::new(),
        }
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }
}

pub fn tick(state: &mut EditorState) {
    let changed: Vec<PathBuf> = match &state.watch {
        Some(session) => session.drain(),
        None => Vec::new(),
    };
    if changed.is_empty() {
        return;
    }

    let mut parents: Vec<PathBuf> = changed
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    parents.sort();
    parents.dedup();

    let fs = state.fs.clone();
    for parent in parents {
        if let Some(node) = find_dir_mut(&mut state.tree.root, &parent)
            && node.loaded
        {
            refresh_dir(node, fs.as_ref());
        }
    }
    sync_open_buffers(state, &changed);
    prune_missing(state);
}

pub fn queue_command(state: &mut EditorState, command: EditorCommand) {
    state.pending_commands.push_back(command);
}

pub fn take_pending_command(state: &mut EditorState) -> Option<EditorCommand> {
    state.pending_commands.pop_front()
}

pub fn has_pending_commands(state: &EditorState) -> bool {
    !state.pending_commands.is_empty()
}

#[cfg(test)]
mod tests;
