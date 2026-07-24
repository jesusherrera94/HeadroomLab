//! State and event handling for the Editor window (the IDE shell).
//!
//! The explorer tree is now **real**: it reads the opened project from disk on
//! demand through `ProjectFileSystemPort`, one directory level per folder
//! expansion. Tabs and the terminal remain static mocks from HL9 (real
//! editor/terminal logic arrives in later tasks).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::application::file_system_service::FileSystemService;
use crate::application::ports::{
    DirEntryInfo, FileWatchSession, FileWatcherPort, ProjectFileSystemPort,
};
use crate::domain::project::RecentProject;

/// Icon shown for a tree node or tab, resolved from the file name / directory
/// state. `Generic` is the required fallback for unknown extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeIcon {
    FolderClosed,
    FolderOpen,
    Cpp,
    Header,
    Build,
    Markdown,
    Json,
    Text,
    Audio,
    Library,
    Git,
    Generic,
}

/// Resolves the icon for a file from its name (full-name specials first, then
/// extension). Directories are handled at render time (open vs closed).
pub fn icon_for_file(name: &str) -> NodeIcon {
    match name {
        "Makefile" | "makefile" | "GNUmakefile" | "CMakeLists.txt" => return NodeIcon::Build,
        ".gitignore" | ".gitattributes" | ".gitmodules" => return NodeIcon::Git,
        _ => {}
    }

    let ext = name
        .rsplit('.')
        .next()
        .filter(|e| *e != name) // no dot → not an extension
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "cpp" | "cc" | "cxx" | "c++" | "c" => NodeIcon::Cpp,
        "h" | "hpp" | "hh" | "hxx" => NodeIcon::Header,
        "mk" | "cmake" => NodeIcon::Build,
        "md" | "markdown" => NodeIcon::Markdown,
        "json" => NodeIcon::Json,
        "txt" => NodeIcon::Text,
        "wav" | "mp3" | "aac" | "ogg" | "flac" => NodeIcon::Audio,
        "dylib" | "so" | "dll" | "a" => NodeIcon::Library,
        _ => NodeIcon::Generic,
    }
}

/// One node in the explorer tree. Directories are read lazily: `children` stays
/// empty until the folder is first expanded (`loaded` flips to `true` then).
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Icon for files; ignored for directories (open/closed chosen at render).
    pub icon: NodeIcon,
    pub loaded: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    fn from_entry(entry: DirEntryInfo) -> Self {
        let icon = if entry.is_dir {
            NodeIcon::FolderClosed
        } else {
            icon_for_file(&entry.name)
        };
        Self {
            name: entry.name,
            path: entry.path,
            is_dir: entry.is_dir,
            icon,
            loaded: false,
            children: Vec::new(),
        }
    }
}

/// The explorer tree rooted at the project folder, plus the selected file.
pub struct FileTreeState {
    pub root: TreeNode,
    pub selected: Option<PathBuf>,
}

impl FileTreeState {
    /// Builds the tree for `project`, eagerly reading (and sorting) the root's
    /// immediate children so the panel isn't empty on first paint. Deeper
    /// folders load on expansion.
    fn new(project: &RecentProject, fs: &dyn ProjectFileSystemPort) -> Self {
        let root = TreeNode {
            name: project.name.clone(),
            path: project.path.clone(),
            is_dir: true,
            icon: NodeIcon::FolderClosed,
            loaded: true,
            children: read_children(fs, &project.path),
        };
        Self {
            root,
            selected: None,
        }
    }
}

/// Reads and sorts one directory level: folders first, then files, each group
/// naturally sorted (see [`natural_cmp`]).
fn read_children(fs: &dyn ProjectFileSystemPort, dir: &Path) -> Vec<TreeNode> {
    let mut nodes: Vec<TreeNode> = fs
        .read_dir(dir)
        .into_iter()
        .map(TreeNode::from_entry)
        .collect();
    nodes.sort_by(cmp_nodes);
    nodes
}

/// Directories before files; within a group, natural case-insensitive by name.
fn cmp_nodes(a: &TreeNode, b: &TreeNode) -> Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| natural_cmp(&a.name, &b.name))
}

/// "Human"/natural comparison: digit runs compare as numbers (`file2` before
/// `file10`), non-digit runs compare case-insensitively, and a digit sorts
/// before a letter (so `[0-9]` names come before `[a-z]` names).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                let (da, db) = (ca.is_ascii_digit(), cb.is_ascii_digit());
                if da && db {
                    let na = take_digits(&mut ai);
                    let nb = take_digits(&mut bi);
                    let ord = cmp_numeric(&na, &nb);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else if da != db {
                    // A digit sorts before a non-digit.
                    return if da {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    };
                } else {
                    let ord = ca.to_ascii_lowercase().cmp(&cb.to_ascii_lowercase());
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

/// Consumes the leading run of digits from `it`.
fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if c.is_ascii_digit() {
            s.push(c);
            it.next();
        } else {
            break;
        }
    }
    s
}

/// Compares two all-digit strings by numeric value, tie-broken so that more
/// leading zeros sort first (stable, overflow-free).
fn cmp_numeric(a: &str, b: &str) -> Ordering {
    let ta = a.trim_start_matches('0');
    let tb = b.trim_start_matches('0');
    ta.len()
        .cmp(&tb.len())
        .then_with(|| ta.cmp(tb))
        .then_with(|| a.len().cmp(&b.len()))
}

/// Finds the directory node at `path` for lazy loading, pruning by path prefix.
fn find_dir_mut<'a>(node: &'a mut TreeNode, path: &Path) -> Option<&'a mut TreeNode> {
    if node.path == path {
        return Some(node);
    }
    for child in &mut node.children {
        if child.is_dir
            && path.starts_with(&child.path)
            && let Some(found) = find_dir_mut(child, path)
        {
            return Some(found);
        }
    }
    None
}

/// The context-aware directory a new entry should be created in: the selected
/// folder, a selected file's parent, or the project root when nothing is
/// selected. Used by the header `+` buttons.
pub fn create_target(tree: &FileTreeState) -> PathBuf {
    match &tree.selected {
        Some(sel) => match find_node(&tree.root, sel) {
            Some(node) if node.is_dir => sel.clone(),
            Some(_) => sel
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| tree.root.path.clone()),
            None => tree.root.path.clone(),
        },
        None => tree.root.path.clone(),
    }
}

/// Finds the node at `path` (file or directory), for reading kind/name.
fn find_node<'a>(node: &'a TreeNode, path: &Path) -> Option<&'a TreeNode> {
    if node.path == path {
        return Some(node);
    }
    if !path.starts_with(&node.path) {
        return None;
    }
    node.children
        .iter()
        .find_map(|child| find_node(child, path))
}

/// Re-reads a loaded directory and merges the result, preserving the
/// `loaded`/`children` (and thus expansion) of sub-directories that still exist.
/// Called both by optimistic in-app refresh and the external watcher.
fn refresh_dir(node: &mut TreeNode, fs: &dyn ProjectFileSystemPort) {
    let fresh = read_children(fs, &node.path);
    let mut old: std::collections::HashMap<PathBuf, TreeNode> = node
        .children
        .drain(..)
        .map(|c| (c.path.clone(), c))
        .collect();

    node.children = fresh
        .into_iter()
        .map(|mut fresh_node| {
            if fresh_node.is_dir
                && let Some(prev) = old.remove(&fresh_node.path)
                && prev.is_dir
            {
                fresh_node.loaded = prev.loaded;
                fresh_node.children = prev.children;
            }
            fresh_node
        })
        .collect();
    node.loaded = true;
}

/// One open buffer shown in the tab strip, backed by a real file path.
pub struct EditorTab {
    pub name: String,
    pub icon: NodeIcon,
    pub unsaved: bool,
    pub path: PathBuf,
}

impl EditorTab {
    fn from_path(path: &Path) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let icon = icon_for_file(&name);
        Self {
            name,
            icon,
            unsaved: false,
            path: path.to_path_buf(),
        }
    }
}

/// Which kind of entry a pending inline "new…" row will create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

/// The icon for a to-be-created entry's inline row.
pub fn entry_kind_icon(kind: EntryKind) -> NodeIcon {
    match kind {
        EntryKind::File => NodeIcon::Generic,
        EntryKind::Directory => NodeIcon::FolderClosed,
    }
}

/// The display icon for a tree row: open/closed folder for directories (per the
/// `open` flag), or the file's own icon (`open` is `None` for files).
pub fn row_icon(node: &TreeNode, open: Option<bool>) -> NodeIcon {
    match open {
        Some(true) => NodeIcon::FolderOpen,
        Some(false) => NodeIcon::FolderClosed,
        None => node.icon,
    }
}

/// An inline "new file/folder" row being typed into `parent`.
pub struct PendingCreate {
    pub parent: PathBuf,
    pub kind: EntryKind,
    pub buffer: String,
    /// Set on open so the render grabs keyboard focus once.
    pub focus: bool,
}

/// An inline rename in progress on `path`.
pub struct PendingRename {
    pub path: PathBuf,
    pub buffer: String,
    pub focus: bool,
}

/// A confirmation the user must resolve before an action runs. Bundles the
/// reusable modal's text with the action to perform on confirm.
pub struct PendingConfirm {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub action: ExplorerAction,
}

/// A deferred, confirmation-gated explorer action.
pub enum ExplorerAction {
    Delete(PathBuf),
}

/// Transient explorer interaction state (inline editors, modal, error banner).
#[derive(Default)]
pub struct ExplorerUiState {
    pub pending_create: Option<PendingCreate>,
    pub pending_rename: Option<PendingRename>,
    pub pending_confirm: Option<PendingConfirm>,
    pub error: Option<String>,
}

/// Per-window state for the Editor. The tree and explorer actions are live; the
/// terminal and code area remain mocks (real editor arrives later).
pub struct EditorState {
    pub project_name: String,
    pub project_path: PathBuf,
    pub fs: Rc<dyn ProjectFileSystemPort>,
    pub fs_service: Rc<FileSystemService>,
    pub watch: Option<Box<dyn FileWatchSession>>,
    pub tree: FileTreeState,
    pub explorer: ExplorerUiState,
    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,
    pub terminal_lines: Vec<String>,
    pub focus_requested: bool,
}

impl EditorState {
    /// Builds the editor state for a freshly opened project: reads the tree,
    /// starts the filesystem watcher, and opens with no tabs.
    pub fn new(
        project: &RecentProject,
        fs: Rc<dyn ProjectFileSystemPort>,
        fs_service: Rc<FileSystemService>,
        watcher: Rc<dyn FileWatcherPort>,
    ) -> Self {
        let tree = FileTreeState::new(project, fs.as_ref());

        let watch = match watcher.watch(&project.path) {
            Ok(session) => Some(session),
            Err(e) => {
                eprintln!("file watcher unavailable, external changes won't sync: {e}");
                None
            }
        };

        let terminal_lines = vec![
            format!("{} $ make dylib", project.name),
            "  (build output will appear here)".to_string(),
        ];

        Self {
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            fs,
            fs_service,
            watch,
            tree,
            explorer: ExplorerUiState::default(),
            tabs: Vec::new(),
            active_tab: 0,
            terminal_lines,
            focus_requested: false,
        }
    }
}

/// What the user did in the explorer this frame.
#[derive(Default)]
pub struct ExplorerEvents {
    /// Folders newly opened this frame that still need their children read.
    pub expand: Vec<PathBuf>,
    /// A file single-clicked (open in a tab + select).
    pub open: Option<PathBuf>,
    /// A folder clicked (selection, for context-aware creation).
    pub select: Option<PathBuf>,
    /// Start an inline "new entry" row inside the given directory.
    pub begin_create: Option<(PathBuf, EntryKind)>,
    pub commit_create: bool,
    pub cancel_create: bool,
    /// Start an inline rename of the given entry.
    pub begin_rename: Option<PathBuf>,
    pub commit_rename: bool,
    pub cancel_rename: bool,
    /// Ask to delete the given entry (routes through the confirm modal).
    pub request_delete: Option<PathBuf>,
    /// Reveal the given entry in the OS file manager.
    pub reveal: Option<PathBuf>,
}

/// What the user did in the Editor view this frame.
#[derive(Default)]
pub struct EditorViewEvents {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    pub tab_clicked: Option<usize>,
    pub tab_closed: Option<usize>,
    pub explorer: ExplorerEvents,
    pub confirm_confirmed: bool,
    pub confirm_cancelled: bool,
    pub error_dismissed: bool,
}

/// Follow-up actions the app must perform after handling the frame's events.
#[derive(Default)]
pub struct EditorRequests {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
}

/// Drains the filesystem watcher and refreshes any loaded directory that
/// changed on disk (from outside the app or from our own actions). Pruning of
/// vanished selection/tabs happens only when something actually changed.
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
    prune_missing(state);
}

/// Applies view events to the state and returns the actions the app must take.
pub fn handle_events(state: &mut EditorState, events: EditorViewEvents) -> EditorRequests {
    if let Some(index) = events.tab_clicked
        && index < state.tabs.len()
    {
        state.active_tab = index;
    }
    if let Some(index) = events.tab_closed {
        close_tab(state, index);
    }

    handle_explorer(state, events.explorer);

    if events.confirm_confirmed {
        run_pending_confirm(state);
    }
    if events.confirm_cancelled {
        state.explorer.pending_confirm = None;
    }
    if events.error_dismissed {
        state.explorer.error = None;
    }

    EditorRequests {
        open_emulator: events.open_emulator,
        build_run: events.build_run,
        compile: events.compile,
    }
}

fn handle_explorer(state: &mut EditorState, events: ExplorerEvents) {
    // Lazily read the children of any folder opened this frame.
    let fs = state.fs.clone();
    for path in events.expand {
        if let Some(node) = find_dir_mut(&mut state.tree.root, &path)
            && !node.loaded
        {
            node.children = read_children(fs.as_ref(), &node.path);
            node.loaded = true;
        }
    }

    if let Some(path) = events.open {
        state.tree.selected = Some(path.clone());
        open_tab(state, &path);
    }
    if let Some(path) = events.select {
        state.tree.selected = Some(path);
    }

    if let Some((parent, kind)) = events.begin_create {
        state.explorer.pending_rename = None;
        state.explorer.pending_create = Some(PendingCreate {
            parent,
            kind,
            buffer: String::new(),
            focus: true,
        });
    }
    if events.cancel_create {
        state.explorer.pending_create = None;
    }
    if events.commit_create {
        commit_create(state);
    }

    if let Some(path) = events.begin_rename {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        state.explorer.pending_create = None;
        state.explorer.pending_rename = Some(PendingRename {
            path,
            buffer: name,
            focus: true,
        });
    }
    if events.cancel_rename {
        state.explorer.pending_rename = None;
    }
    if events.commit_rename {
        commit_rename(state);
    }

    if let Some(path) = events.request_delete {
        request_delete(state, &path);
    }
    if let Some(path) = events.reveal
        && let Err(e) = state.fs_service.reveal(&path)
    {
        state.explorer.error = Some(e.to_string());
    }
}

fn commit_create(state: &mut EditorState) {
    let Some(pending) = state.explorer.pending_create.as_ref() else {
        return;
    };
    let (parent, kind, name) = (pending.parent.clone(), pending.kind, pending.buffer.clone());

    let result = match kind {
        EntryKind::File => state.fs_service.create_file(&parent, &name),
        EntryKind::Directory => state.fs_service.create_directory(&parent, &name),
    };
    match result {
        Ok(new_path) => {
            state.explorer.pending_create = None;
            refresh_parent(state, &parent);
            state.tree.selected = Some(new_path);
        }
        // Keep the inline row open so the user can fix the name.
        Err(e) => state.explorer.error = Some(e.to_string()),
    }
}

fn commit_rename(state: &mut EditorState) {
    let Some(pending) = state.explorer.pending_rename.as_ref() else {
        return;
    };
    let (old_path, name) = (pending.path.clone(), pending.buffer.clone());

    match state.fs_service.rename(&old_path, &name) {
        Ok(new_path) => {
            state.explorer.pending_rename = None;
            if let Some(parent) = old_path.parent() {
                refresh_parent(state, parent);
            }
            retarget_after_rename(state, &old_path, &new_path);
        }
        Err(e) => state.explorer.error = Some(e.to_string()),
    }
}

fn request_delete(state: &mut EditorState, path: &Path) {
    let Some(node) = find_node(&state.tree.root, path) else {
        return;
    };
    let (title, message) = if node.is_dir {
        (
            "Delete directory",
            format!(
                "Delete \"{}\" and all its contents? It will be moved to the Trash.",
                node.name
            ),
        )
    } else {
        (
            "Delete file",
            format!("Delete \"{}\"? It will be moved to the Trash.", node.name),
        )
    };
    state.explorer.pending_confirm = Some(PendingConfirm {
        title: title.to_string(),
        message,
        confirm_label: "Delete".to_string(),
        action: ExplorerAction::Delete(path.to_path_buf()),
    });
}

fn run_pending_confirm(state: &mut EditorState) {
    let Some(confirm) = state.explorer.pending_confirm.take() else {
        return;
    };
    match confirm.action {
        ExplorerAction::Delete(path) => match state.fs_service.delete(&path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    refresh_parent(state, parent);
                }
                prune_missing(state);
            }
            Err(e) => state.explorer.error = Some(e.to_string()),
        },
    }
}

/// Re-reads `dir` in the tree if it's currently loaded (optimistic refresh).
fn refresh_parent(state: &mut EditorState, dir: &Path) {
    let fs = state.fs.clone();
    if let Some(node) = find_dir_mut(&mut state.tree.root, dir)
        && node.loaded
    {
        refresh_dir(node, fs.as_ref());
    }
}

/// Opens (or re-activates) a tab for `path`.
fn open_tab(state: &mut EditorState, path: &Path) {
    if let Some(index) = state.tabs.iter().position(|t| t.path == path) {
        state.active_tab = index;
    } else {
        state.tabs.push(EditorTab::from_path(path));
        state.active_tab = state.tabs.len() - 1;
    }
}

fn close_tab(state: &mut EditorState, index: usize) {
    if index >= state.tabs.len() {
        return;
    }
    state.tabs.remove(index);
    if state.active_tab > index {
        state.active_tab -= 1;
    }
    if state.active_tab >= state.tabs.len() {
        state.active_tab = state.tabs.len().saturating_sub(1);
    }
}

/// After a rename, follow the moved path in the selection and any open tabs
/// (including files nested under a renamed directory).
fn retarget_after_rename(state: &mut EditorState, old: &Path, new: &Path) {
    if let Some(sel) = state.tree.selected.clone() {
        if sel == old {
            state.tree.selected = Some(new.to_path_buf());
        } else if let Ok(rest) = sel.strip_prefix(old) {
            state.tree.selected = Some(new.join(rest));
        }
    }
    for tab in &mut state.tabs {
        if tab.path == old {
            *tab = EditorTab::from_path(new);
        } else if let Ok(rest) = tab.path.strip_prefix(old) {
            *tab = EditorTab::from_path(&new.join(rest));
        }
    }
}

/// Drops selection and tabs whose backing file no longer exists.
fn prune_missing(state: &mut EditorState) {
    if let Some(sel) = state.tree.selected.clone()
        && !state.fs.exists(&sel)
    {
        state.tree.selected = None;
    }
    state.tabs.retain(|t| state.fs.exists(&t.path));
    if state.active_tab >= state.tabs.len() {
        state.active_tab = state.tabs.len().saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    #[test]
    fn numbers_sort_before_letters() {
        assert_eq!(
            sorted(&["apple", "1file", "banana"]),
            ["1file", "apple", "banana"]
        );
    }

    #[test]
    fn numeric_runs_compare_as_numbers() {
        assert_eq!(
            sorted(&["file10", "file2", "file1"]),
            ["file1", "file2", "file10"]
        );
    }

    #[test]
    fn comparison_is_case_insensitive() {
        assert_eq!(
            sorted(&["Zeta", "alpha", "Beta"]),
            ["alpha", "Beta", "Zeta"]
        );
    }

    #[test]
    fn folders_sort_before_files() {
        let dir = |name: &str, is_dir: bool| TreeNode {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir,
            icon: NodeIcon::Generic,
            loaded: false,
            children: Vec::new(),
        };
        let mut nodes = [
            dir("main.cpp", false),
            dir("src", true),
            dir("README.md", false),
            dir("include", true),
        ];
        nodes.sort_by(cmp_nodes);
        let order: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(order, ["include", "src", "main.cpp", "README.md"]);
    }

    #[test]
    fn icon_resolution_covers_specials_and_fallback() {
        assert_eq!(icon_for_file("Makefile"), NodeIcon::Build);
        assert_eq!(icon_for_file(".gitignore"), NodeIcon::Git);
        assert_eq!(icon_for_file("effect.cpp"), NodeIcon::Cpp);
        assert_eq!(icon_for_file("effect.h"), NodeIcon::Header);
        assert_eq!(icon_for_file("notes.md"), NodeIcon::Markdown);
        assert_eq!(icon_for_file("mystery.xyz"), NodeIcon::Generic);
        assert_eq!(icon_for_file("README"), NodeIcon::Generic);
    }

    /// Fake filesystem returning a fixed listing per directory, for `refresh_dir`.
    struct FakeFs {
        listing: std::collections::HashMap<PathBuf, Vec<(String, bool)>>,
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
        fn exists(&self, _: &Path) -> bool {
            true
        }
        fn create_file(
            &self,
            _: &Path,
            _: &str,
        ) -> Result<PathBuf, crate::domain::file_system::FileSystemError> {
            unreachable!()
        }
        fn create_dir(
            &self,
            _: &Path,
            _: &str,
        ) -> Result<PathBuf, crate::domain::file_system::FileSystemError> {
            unreachable!()
        }
        fn rename(
            &self,
            _: &Path,
            _: &str,
        ) -> Result<PathBuf, crate::domain::file_system::FileSystemError> {
            unreachable!()
        }
        fn delete_to_trash(
            &self,
            _: &Path,
        ) -> Result<(), crate::domain::file_system::FileSystemError> {
            unreachable!()
        }
        fn reveal(&self, _: &Path) -> Result<(), crate::domain::file_system::FileSystemError> {
            unreachable!()
        }
    }

    #[test]
    fn refresh_dir_preserves_loaded_subtree_and_adds_new_entries() {
        let root = PathBuf::from("/proj");
        let mut listing = std::collections::HashMap::new();
        // Root gains a new file "b.txt" alongside the existing "src" dir.
        listing.insert(
            root.clone(),
            vec![("src".into(), true), ("b.txt".into(), false)],
        );
        let fs = FakeFs { listing };

        // Existing tree: root → [src (loaded, with a child)].
        let mut src = TreeNode {
            name: "src".into(),
            path: root.join("src"),
            is_dir: true,
            icon: NodeIcon::FolderClosed,
            loaded: true,
            children: vec![TreeNode {
                name: "main.cpp".into(),
                path: root.join("src/main.cpp"),
                is_dir: false,
                icon: NodeIcon::Cpp,
                loaded: false,
                children: Vec::new(),
            }],
        };
        src.loaded = true;
        let mut root_node = TreeNode {
            name: "proj".into(),
            path: root.clone(),
            is_dir: true,
            icon: NodeIcon::FolderClosed,
            loaded: true,
            children: vec![src],
        };

        refresh_dir(&mut root_node, &fs);

        // Folders-first, then the new file; and src keeps its loaded child.
        let names: Vec<&str> = root_node.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["src", "b.txt"]);
        let src = &root_node.children[0];
        assert!(src.loaded);
        assert_eq!(src.children.len(), 1);
        assert_eq!(src.children[0].name, "main.cpp");
    }
}
