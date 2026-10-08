//! Shared fakes and fixtures for the editor controller's tests.

mod documents;
mod tab_lifecycle;
mod tree;

use std::collections::{HashMap, HashSet};

use super::confirm::run_pending_confirm;
use super::file_tree::cmp_nodes;
use super::tabs::{close_tab, open_tab, request_close_tab, retarget_after_rename, save_tab};
use super::*;
use crate::application::ports::DirEntryInfo;
use crate::domain::file_system::FileSystemError;
use crate::domain::text_document::DocumentContent;

struct NoTerminal;

impl crate::application::ports::TerminalPort for NoTerminal {
    fn open(
        &self,
        _shell: &crate::domain::terminal::ShellChoice,
        _cwd: &Path,
        _size: crate::domain::terminal::TerminalSize,
        _palette: crate::domain::terminal::TerminalPalette,
    ) -> Result<
        Box<dyn crate::application::ports::TerminalSession>,
        crate::application::ports::TerminalError,
    > {
        Err(crate::application::ports::TerminalError(
            "no terminal in tests".into(),
        ))
    }
}

struct FakeClipboard;

impl ClipboardPort for FakeClipboard {
    fn read(&self) -> Option<String> {
        None
    }
}

#[derive(Default)]
struct FakeFs {
    listing: HashMap<PathBuf, Vec<(String, bool)>>,
    files: std::cell::RefCell<HashMap<PathBuf, Vec<u8>>>,
    clock: std::cell::Cell<u64>,
    stamps: std::cell::RefCell<HashMap<PathBuf, std::time::SystemTime>>,
}

impl FakeFs {
    fn with_file(path: &Path, contents: &[u8]) -> Self {
        let fs = FakeFs::default();
        fs.write_file(path, contents).unwrap();
        fs
    }
}

impl ProjectFileSystemPort for FakeFs {
    fn read_dir(&self, dir: &Path) -> Vec<DirEntryInfo> {
        self.listing
            .get(dir)
            .into_iter()
            .flatten()
            .map(|(name, is_dir)| DirEntryInfo {
                name: name.clone(),
                path: dir.join(name),
                is_dir: *is_dir,
            })
            .collect()
    }
    fn exists(&self, path: &Path) -> bool {
        // Directories only ever come from `listing`; files from the store.
        self.listing.contains_key(path) || self.files.borrow().contains_key(path)
    }
    fn create_file(&self, _: &Path, _: &str) -> Result<PathBuf, FileSystemError> {
        unreachable!()
    }
    fn create_dir(&self, _: &Path, _: &str) -> Result<PathBuf, FileSystemError> {
        unreachable!()
    }
    fn rename(&self, _: &Path, _: &str) -> Result<PathBuf, FileSystemError> {
        unreachable!()
    }
    fn delete_to_trash(&self, _: &Path) -> Result<(), FileSystemError> {
        unreachable!()
    }
    fn reveal(&self, _: &Path) -> Result<(), FileSystemError> {
        unreachable!()
    }

    fn read_file(&self, path: &Path) -> Result<Vec<u8>, FileSystemError> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| FileSystemError::NotFound(path.display().to_string()))
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), contents.to_vec());
        let tick = self.clock.get() + 1;
        self.clock.set(tick);
        self.stamps.borrow_mut().insert(
            path.to_path_buf(),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(tick),
        );
        Ok(())
    }

    fn modified(&self, path: &Path) -> Option<std::time::SystemTime> {
        self.stamps.borrow().get(path).copied()
    }
}

fn state_over(fs: Rc<FakeFs>) -> EditorState {
    let fs_service = Rc::new(FileSystemService::new(fs.clone()));
    let root = PathBuf::from("/proj");
    EditorState {
        project_name: "proj".into(),
        project_path: root.clone(),
        fs: fs.clone(),
        fs_service,
        clipboard: Rc::new(FakeClipboard),
        watch: None,
        pending_commands: VecDeque::new(),
        tree: FileTreeState {
            root: TreeNode {
                name: "proj".into(),
                path: root,
                is_dir: true,
                icon: NodeIcon::FolderClosed,
                loaded: true,
                children: Vec::new(),
            },
            selected: None,
        },
        explorer: ExplorerUiState::default(),
        tabs: Vec::new(),
        active_tab: 0,
        next_tab_id: 0,
        discarded_tabs: Vec::new(),
        scroll_active_into_view: false,
        cursor: None,
        terminal: TerminalState::new(
            Rc::new(NoTerminal),
            Rc::new(FakeClipboard),
            crate::presentation::theme::terminal_palette(),
            Path::new("/proj"),
        ),
        focus_requested: false,
    }
}

fn tab_id(state: &EditorState, index: usize) -> TabId {
    state.tabs[index].id
}

fn edit_active(state: &mut EditorState, text: &str) {
    if let DocumentContent::Text { text: buffer, .. } = &mut state.tabs[state.active_tab].content {
        *buffer = text.to_string();
    }
}

fn state_with_tabs(names: &[&str]) -> (Rc<FakeFs>, EditorState) {
    let fs = Rc::new(FakeFs::default());
    for name in names {
        fs.write_file(&PathBuf::from("/proj").join(name), name.as_bytes())
            .unwrap();
    }
    let mut state = state_over(fs.clone());
    for name in names {
        open_tab(&mut state, &PathBuf::from("/proj").join(name));
    }
    (fs, state)
}

fn tab_names(state: &EditorState) -> Vec<&str> {
    state.tabs.iter().map(|t| t.name.as_str()).collect()
}
