//! State and event handling for the Editor window (the IDE shell).
//!
//! The explorer tree reads the opened project from disk on demand through
//! `ProjectFileSystemPort`, one directory level per folder expansion. Tabs are
//! now real buffers: opening a file reads and classifies it, typing marks it
//! dirty, and Cmd/Ctrl+S writes it back. Only the terminal remains a mock.
//!
//! Three invariants here exist to protect unsaved work, and all are easy to
//! break by accident:
//!
//! * `prune_missing` must not drop a **dirty** tab whose file vanished — that
//!   buffer is the last copy of the user's work, and saving recreates the file.
//! * `retarget_after_rename` must move a tab's path **in place** rather than
//!   rebuilding it from disk, which would silently discard the buffer.
//! * Anything that outlives the frame it was raised in — the close/quit
//!   confirmation especially — must refer to a tab by [`TabId`], never by its
//!   position. Tabs can be reordered by dragging and pruned by the watcher, so a
//!   stored index silently retargets and would discard the wrong buffer.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use crate::application::file_system_service::FileSystemService;
use crate::application::ports::{
    ClipboardPort, DirEntryInfo, FileWatchSession, FileWatcherPort, ProjectFileSystemPort,
};
use crate::domain::file_system::FileSystemError;
use crate::domain::project::RecentProject;
use crate::domain::text_document::{DocumentContent, Language, language_for};
use crate::presentation::components::molecules::find_bar::FindState;

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

/// A tab's identity, stable for as long as the tab is open.
///
/// Positions are not identities here: the strip can be reordered by dragging and
/// pruned by the file watcher, so anything that refers to a tab across frames
/// must hold one of these. Handed out by `EditorState::next_tab_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

/// One open buffer shown in the tab strip, backed by a real file on disk.
pub struct EditorTab {
    pub id: TabId,
    pub name: String,
    pub icon: NodeIcon,
    pub path: PathBuf,
    pub language: Language,
    /// The live buffer, or the reason this file isn't editable.
    pub content: DocumentContent,
    /// The text as last read from / written to disk. Dirtiness is this compared
    /// against the buffer, not an "was edited" flag, so typing a change and
    /// undoing it correctly clears the ● dot.
    pub saved_text: String,
    /// Modification time as of the last read/write. Lets `tick` tell an external
    /// edit apart from the watcher event our own save just caused.
    pub disk_modified: Option<SystemTime>,
    /// Set when the file changed on disk under a dirty buffer, so we kept the
    /// user's text instead of reloading over it.
    pub external_change: bool,
    /// Set when the buffer was replaced from disk, so the next paint drops the
    /// undo history with it. Without this, `⌘Z` would rewind past the reload and
    /// restore text for a file that has since moved underneath.
    pub history_reset: bool,
    /// Live find state, present only while the find bar is open.
    pub find: Option<FindState>,
}

impl EditorTab {
    /// Opens `path` through the service, classifying the bytes on the way in.
    fn load(
        id: TabId,
        path: &Path,
        fs_service: &FileSystemService,
    ) -> Result<Self, FileSystemError> {
        let opened = fs_service.open_document(path)?;
        Ok(Self::from_parts(id, path, opened.content, opened.modified))
    }

    fn from_parts(
        id: TabId,
        path: &Path,
        content: DocumentContent,
        disk_modified: Option<SystemTime>,
    ) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            id,
            icon: icon_for_file(&name),
            language: language_for(&name),
            saved_text: content.text().unwrap_or_default().to_owned(),
            name,
            path: path.to_path_buf(),
            content,
            disk_modified,
            external_change: false,
            history_reset: false,
            find: None,
        }
    }

    /// Whether the buffer differs from what is on disk. Non-text documents are
    /// never dirty — they cannot be edited in the first place.
    pub fn unsaved(&self) -> bool {
        match self.content.text() {
            Some(text) => text != self.saved_text,
            None => false,
        }
    }

    /// Moves this tab to a new path (a rename), keeping the buffer — including
    /// unsaved edits — intact.
    fn retarget(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.icon = icon_for_file(&name);
        self.language = language_for(&name);
        self.name = name;
        self.path = path.to_path_buf();
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

/// The platform's name for "show this in the OS file manager". Shared by the
/// explorer's context menu and the tab strip's, so the two can't drift apart.
pub fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in Explorer"
    } else {
        "Show in Files"
    }
}

/// How the save shortcut is written for this platform, for tooltips and the
/// status bar's "… to save" hint.
pub fn save_shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘S"
    } else {
        "Ctrl+S"
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
    /// Optional third button, for the "save instead of discarding" choices.
    /// `None` keeps the original two-button modal used by delete.
    pub alternate_label: Option<String>,
    pub action: EditorAction,
}

/// Which button of the confirmation the user pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    /// The primary (destructive) action: delete, discard, quit.
    Primary,
    /// The alternate action: save first, then proceed.
    Alternate,
}

/// A deferred, confirmation-gated action.
///
/// The tab is named by [`TabId`] rather than by position precisely because this
/// outlives the frame that raised it: the watcher can prune a tab, and the user
/// can drag the strip into a new order, while the modal is up.
pub enum EditorAction {
    Delete(PathBuf),
    /// Close a tab whose buffer has unsaved changes.
    CloseTab(TabId),
    /// Quit with unsaved buffers open.
    Quit,
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
    /// Read side of the system clipboard, for the code editor's Paste menu item.
    /// Writes go through egui's own `Context::copy_text`.
    pub clipboard: Rc<dyn ClipboardPort>,
    pub watch: Option<Box<dyn FileWatchSession>>,
    pub tree: FileTreeState,
    pub explorer: ExplorerUiState,
    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,
    /// Source of [`TabId`]s, bumped once per tab opened. Never reused, so a stale
    /// id can only ever fail to resolve — it can never resolve to a *different*
    /// buffer than the one it was taken for.
    next_tab_id: u64,
    /// Tabs that went away this frame, however they went (closed, or pruned by
    /// the watcher). The view drains this to drop each one's `TextEditState`
    /// from egui's memory map, which has no GC of its own — without it every
    /// closed tab leaks its buffer snapshot for the life of the process.
    discarded_tabs: Vec<TabId>,
    /// Set when the active tab changed from outside the strip (opening a file, a
    /// reorder), so the next paint scrolls it back into view.
    pub scroll_active_into_view: bool,
    /// Last known 1-based cursor position in the code area, for the status bar.
    pub cursor: Option<(usize, usize)>,
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
        clipboard: Rc<dyn ClipboardPort>,
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
            clipboard,
            watch,
            tree,
            explorer: ExplorerUiState::default(),
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            discarded_tabs: Vec::new(),
            scroll_active_into_view: false,
            cursor: None,
            terminal_lines,
            focus_requested: false,
        }
    }

    /// Takes the next unused tab identity.
    fn take_tab_id(&mut self) -> TabId {
        self.next_tab_id += 1;
        TabId(self.next_tab_id)
    }

    /// The current position of the tab with `id`, or `None` if it has since been
    /// closed or pruned.
    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }
}

/// Takes the tabs that went away since the last call, so the view can drop each
/// one's editor state. Drained rather than read, so an id is acted on once.
pub fn take_discarded_tabs(state: &mut EditorState) -> Vec<TabId> {
    std::mem::take(&mut state.discarded_tabs)
}

/// The paths of every open buffer with unsaved changes — what the Explorer needs
/// to mark its rows, without knowing anything about tabs.
pub fn unsaved_paths(state: &EditorState) -> HashSet<PathBuf> {
    state
        .tabs
        .iter()
        .filter(|t| t.unsaved())
        .map(|t| t.path.clone())
        .collect()
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

/// What the user did in the code area this frame.
#[derive(Default)]
pub struct CodeEvents {
    /// The active buffer was typed into.
    pub edited: bool,
    /// Live cursor position, for the status bar.
    pub cursor: Option<(usize, usize)>,
    pub save: bool,
    pub save_all: bool,
    /// Reload the active buffer from disk, discarding edits.
    pub reload: bool,
    pub open_find: bool,
    pub close_find: bool,
}

/// What the user did in the Editor view this frame.
///
/// `tab_clicked` is positional because it is applied in the same frame it was
/// produced; everything that can survive into a later frame carries a [`TabId`].
#[derive(Default)]
pub struct EditorViewEvents {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    pub tab_clicked: Option<usize>,
    pub tab_closed: Option<TabId>,
    /// Save one specific tab — the ● in the tab strip, or its context menu.
    pub tab_saved: Option<TabId>,
    /// A tab was dropped: which tab was dragged, and the slot it landed on in
    /// the index space the strip was painted in. Identity resolution stays in
    /// this module — the view never converts a `TabId` to a position.
    pub tab_reordered: Option<(TabId, usize)>,
    /// Reveal a tab's file in the OS file manager.
    pub tab_reveal: Option<PathBuf>,
    pub explorer: ExplorerEvents,
    pub code: CodeEvents,
    pub confirm_confirmed: bool,
    pub confirm_alternate: bool,
    pub confirm_cancelled: bool,
    pub error_dismissed: bool,
}

/// Follow-up actions the app must perform after handling the frame's events.
#[derive(Default)]
pub struct EditorRequests {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    /// A quit was confirmed despite unsaved buffers — let the close through.
    pub quit_confirmed: bool,
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
    sync_open_buffers(state, &changed);
    prune_missing(state);
}

/// Reconciles open buffers with files that changed on disk.
///
/// A clean buffer is silently reloaded, so the editor always shows the truth. A
/// dirty buffer is left exactly as the user typed it and merely flagged — losing
/// someone's unsaved work to a background `git checkout` or a build would be
/// indefensible. Our own saves are filtered out by comparing the recorded mtime.
fn sync_open_buffers(state: &mut EditorState, changed: &[PathBuf]) {
    let fs_service = state.fs_service.clone();

    for tab in &mut state.tabs {
        if !changed.iter().any(|p| p == &tab.path) {
            continue;
        }
        // The mtime we stored when we last read or wrote this file. If disk
        // still matches it, this event is the echo of our own save.
        if state.fs.modified(&tab.path) == tab.disk_modified {
            continue;
        }

        if tab.unsaved() {
            tab.external_change = true;
            continue;
        }
        if let Ok(opened) = fs_service.open_document(&tab.path) {
            tab.saved_text = opened.content.text().unwrap_or_default().to_owned();
            tab.content = opened.content;
            tab.disk_modified = opened.modified;
            tab.external_change = false;
            tab.history_reset = true;
        }
    }
}

/// Applies view events to the state and returns the actions the app must take.
pub fn handle_events(state: &mut EditorState, events: EditorViewEvents) -> EditorRequests {
    if let Some(index) = events.tab_clicked
        && index < state.tabs.len()
    {
        state.active_tab = index;
    }
    if let Some((id, insert_before)) = events.tab_reordered
        && let Some(from) = state.index_of(id)
    {
        move_tab(state, from, insert_before);
    }
    if let Some(id) = events.tab_saved
        && let Some(index) = state.index_of(id)
    {
        // Deliberately not `state.active_tab = index`: the ● is an action button,
        // and yanking the code pane to another file as a side effect of "save
        // this" is the kind of surprise this story exists to remove.
        save_tab(state, index);
    }
    if let Some(id) = events.tab_closed {
        request_close_tab(state, id);
    }
    if let Some(path) = events.tab_reveal
        && let Err(e) = state.fs_service.reveal(&path)
    {
        state.explorer.error = Some(e.to_string());
    }

    handle_explorer(state, events.explorer);
    handle_code(state, events.code);

    let mut quit_confirmed = false;
    if events.confirm_confirmed {
        quit_confirmed = run_pending_confirm(state, ConfirmChoice::Primary);
    }
    if events.confirm_alternate {
        quit_confirmed = run_pending_confirm(state, ConfirmChoice::Alternate);
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
        quit_confirmed,
    }
}

fn handle_code(state: &mut EditorState, events: CodeEvents) {
    if events.save {
        save_active(state);
    }
    if events.save_all {
        save_all(state);
    }
    if events.reload {
        reload_active(state);
    }
    if let Some(tab) = state.tabs.get_mut(state.active_tab) {
        if events.open_find && tab.content.is_editable() {
            tab.find = Some(FindState::default());
        }
        if events.close_find {
            tab.find = None;
        }
    }
}

/// Writes the active buffer to disk. On success the saved snapshot and mtime are
/// updated *together*, which is what makes the next watcher event a no-op.
fn save_active(state: &mut EditorState) {
    save_tab(state, state.active_tab);
}

fn save_all(state: &mut EditorState) {
    for index in 0..state.tabs.len() {
        if state.tabs[index].unsaved() {
            save_tab(state, index);
        }
    }
}

fn save_tab(state: &mut EditorState, index: usize) -> bool {
    let Some(tab) = state.tabs.get(index) else {
        return false;
    };
    let DocumentContent::Text { text, crlf, .. } = &tab.content else {
        return false; // Binary / oversized documents are never savable.
    };
    let (path, text, crlf) = (tab.path.clone(), text.clone(), *crlf);

    match state.fs_service.save_document(&path, &text, crlf) {
        Ok(modified) => {
            let tab = &mut state.tabs[index];
            tab.saved_text = text;
            tab.disk_modified = modified;
            tab.external_change = false;
            true
        }
        Err(e) => {
            // The buffer stays dirty, so nothing is lost by a failed write.
            state.explorer.error = Some(e.to_string());
            false
        }
    }
}

/// Re-reads the active tab from disk, discarding the buffer.
fn reload_active(state: &mut EditorState) {
    let Some(tab) = state.tabs.get(state.active_tab) else {
        return;
    };
    let path = tab.path.clone();
    match state.fs_service.open_document(&path) {
        Ok(opened) => {
            let tab = &mut state.tabs[state.active_tab];
            tab.saved_text = opened.content.text().unwrap_or_default().to_owned();
            tab.content = opened.content;
            tab.disk_modified = opened.modified;
            tab.external_change = false;
            tab.history_reset = true;
        }
        Err(e) => state.explorer.error = Some(e.to_string()),
    }
}

/// True when any open buffer has unsaved changes — the quit guard's condition.
pub fn has_unsaved_work(state: &EditorState) -> bool {
    state.tabs.iter().any(EditorTab::unsaved)
}

/// Raises the quit confirmation. Returns false when there is nothing to guard,
/// in which case the caller should just let the window close.
pub fn request_quit(state: &mut EditorState) -> bool {
    if !has_unsaved_work(state) {
        return false;
    }
    let count = state.tabs.iter().filter(|t| t.unsaved()).count();
    let message = if count == 1 {
        "1 file has unsaved changes. Save before quitting?".to_owned()
    } else {
        format!("{count} files have unsaved changes. Save before quitting?")
    };
    state.explorer.pending_confirm = Some(PendingConfirm {
        title: "Unsaved changes".to_string(),
        message,
        confirm_label: "Discard & quit".to_string(),
        alternate_label: Some("Save all & quit".to_string()),
        action: EditorAction::Quit,
    });
    true
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
        alternate_label: None,
        action: EditorAction::Delete(path.to_path_buf()),
    });
}

/// Runs the pending confirmation. Returns true when a quit was approved, so the
/// caller can let the window close.
fn run_pending_confirm(state: &mut EditorState, choice: ConfirmChoice) -> bool {
    let Some(confirm) = state.explorer.pending_confirm.take() else {
        return false;
    };
    match confirm.action {
        EditorAction::Delete(path) => {
            match state.fs_service.delete(&path) {
                Ok(()) => {
                    if let Some(parent) = path.parent() {
                        refresh_parent(state, parent);
                    }
                    prune_missing(state);
                }
                Err(e) => state.explorer.error = Some(e.to_string()),
            }
            false
        }
        EditorAction::CloseTab(id) => {
            // Resolved now, not when the confirmation was raised: the strip may
            // have been reordered or pruned while the modal was up. A tab that
            // has since gone away needs no closing.
            let Some(index) = state.index_of(id) else {
                return false;
            };
            // "Save" must not close a tab whose write failed — the error banner
            // is shown and the buffer stays open and dirty.
            if choice == ConfirmChoice::Alternate && !save_tab(state, index) {
                return false;
            }
            close_tab(state, index);
            false
        }
        EditorAction::Quit => {
            if choice == ConfirmChoice::Alternate {
                save_all(state);
                // Any write that failed left its buffer dirty; don't quit over it.
                if has_unsaved_work(state) {
                    return false;
                }
            }
            true
        }
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

/// Opens (or re-activates) a tab for `path`, reading its content from disk.
/// A read failure surfaces in the error banner and opens no tab.
fn open_tab(state: &mut EditorState, path: &Path) {
    state.scroll_active_into_view = true;
    if let Some(index) = state.tabs.iter().position(|t| t.path == path) {
        state.active_tab = index;
        return;
    }
    let id = state.take_tab_id();
    match EditorTab::load(id, path, &state.fs_service) {
        Ok(tab) => {
            state.tabs.push(tab);
            state.active_tab = state.tabs.len() - 1;
        }
        Err(e) => state.explorer.error = Some(e.to_string()),
    }
}

/// Closing a tab is confirmation-gated when its buffer is dirty.
fn request_close_tab(state: &mut EditorState, id: TabId) {
    let Some(index) = state.index_of(id) else {
        return;
    };
    let tab = &state.tabs[index];
    if !tab.unsaved() {
        close_tab(state, index);
        return;
    }
    state.explorer.pending_confirm = Some(PendingConfirm {
        title: "Unsaved changes".to_string(),
        message: format!("\"{}\" has unsaved changes.", tab.name),
        confirm_label: "Discard".to_string(),
        alternate_label: Some("Save".to_string()),
        action: EditorAction::CloseTab(id),
    });
}

/// Moves the tab at `from` so that it lands immediately before what is currently
/// at `insert_before` — the index space the strip was painted in, so
/// `insert_before == state.tabs.len()` means "drop at the end".
///
/// The previously active buffer stays active: `active_tab` is re-derived from its
/// [`TabId`] rather than assumed to still be the same slot.
pub fn move_tab(state: &mut EditorState, from: usize, insert_before: usize) {
    let len = state.tabs.len();
    if from >= len || insert_before > len {
        return;
    }
    // Removing `from` shifts everything after it down by one, so a landing slot
    // past the dragged tab has to come back by one to mean the same gap.
    let to = if insert_before > from {
        insert_before - 1
    } else {
        insert_before
    };
    if to == from {
        return;
    }

    let active_id = state.tabs.get(state.active_tab).map(|t| t.id);
    let tab = state.tabs.remove(from);
    state.tabs.insert(to, tab);

    if let Some(index) = active_id.and_then(|id| state.index_of(id)) {
        state.active_tab = index;
    }
    state.scroll_active_into_view = true;
}

fn close_tab(state: &mut EditorState, index: usize) {
    if index >= state.tabs.len() {
        return;
    }
    let gone = state.tabs.remove(index);
    state.discarded_tabs.push(gone.id);
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
    // Retarget in place rather than rebuilding the tab: a rebuild would re-read
    // the file and silently throw away whatever the user had typed.
    for tab in &mut state.tabs {
        if tab.path == old {
            tab.retarget(new);
        } else if let Ok(rest) = tab.path.strip_prefix(old) {
            let moved = new.join(rest);
            tab.retarget(&moved);
        }
    }
}

/// Drops selection and tabs whose backing file no longer exists.
///
/// A **dirty** tab survives deletion: its buffer is the only remaining copy of
/// the user's work, and saving it recreates the file.
fn prune_missing(state: &mut EditorState) {
    if let Some(sel) = state.tree.selected.clone()
        && !state.fs.exists(&sel)
    {
        state.tree.selected = None;
    }
    state.tabs.retain(|t| {
        let keep = t.unsaved() || state.fs.exists(&t.path);
        if !keep {
            state.discarded_tabs.push(t.id);
        }
        keep
    });
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

    /// Always-empty clipboard. Nothing in this module reads it — Paste lives in
    /// the code editor's context menu — but `EditorState` carries the port, so
    /// the tests need something to hold.
    struct FakeClipboard;

    impl ClipboardPort for FakeClipboard {
        fn read(&self) -> Option<String> {
            None
        }
    }

    /// Fake filesystem returning a fixed listing per directory, plus an in-memory
    /// file store so buffer loading and saving can be exercised without disk.
    #[derive(Default)]
    struct FakeFs {
        listing: std::collections::HashMap<PathBuf, Vec<(String, bool)>>,
        files: std::cell::RefCell<std::collections::HashMap<PathBuf, Vec<u8>>>,
        /// Bumped on every write so `modified` returns a fresh instant, the way a
        /// real filesystem would.
        clock: std::cell::Cell<u64>,
        stamps: std::cell::RefCell<std::collections::HashMap<PathBuf, std::time::SystemTime>>,
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

        fn read_file(
            &self,
            path: &Path,
        ) -> Result<Vec<u8>, crate::domain::file_system::FileSystemError> {
            self.files.borrow().get(path).cloned().ok_or_else(|| {
                crate::domain::file_system::FileSystemError::NotFound(path.display().to_string())
            })
        }

        fn write_file(
            &self,
            path: &Path,
            contents: &[u8],
        ) -> Result<(), crate::domain::file_system::FileSystemError> {
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

    #[test]
    fn open_document_classifies_and_save_round_trips_crlf() {
        use crate::domain::text_document::DocumentContent;

        let path = PathBuf::from("/proj/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"int a;\r\nint b;\r\n"));
        let service = FileSystemService::new(fs.clone());

        let opened = service.open_document(&path).expect("readable");
        let DocumentContent::Text { text, crlf, .. } = opened.content else {
            panic!("expected editable text");
        };
        // CRLF is normalised for editing…
        assert_eq!(text, "int a;\nint b;\n");
        assert!(crlf);

        // …and restored byte-for-byte on the way back out.
        let stamp = service.save_document(&path, &text, crlf).expect("writable");
        assert_eq!(
            fs.read_file(&path).unwrap(),
            b"int a;\r\nint b;\r\n".to_vec()
        );
        // The post-write mtime is handed back so the caller can ignore its own
        // watcher event.
        assert!(stamp.is_some());
        assert_ne!(stamp, opened.modified);
    }

    #[test]
    fn open_document_reports_binary_and_missing_files() {
        use crate::domain::text_document::DocumentContent;

        let path = PathBuf::from("/proj/build/libx.dylib");
        let fs = Rc::new(FakeFs::with_file(&path, &[0x00, 0xFF, 0xFE]));
        let service = FileSystemService::new(fs);

        assert_eq!(
            service.open_document(&path).unwrap().content,
            DocumentContent::Binary
        );
        assert!(matches!(
            service.open_document(Path::new("/proj/gone.cpp")),
            Err(crate::domain::file_system::FileSystemError::NotFound(_))
        ));
    }

    /// Builds an editor state over `fs` with no watcher, rooted at `/proj`.
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
            terminal_lines: Vec::new(),
            focus_requested: false,
        }
    }

    /// The identity of the tab currently at `index`. Tests name tabs by identity
    /// for the same reason the production code does.
    fn tab_id(state: &EditorState, index: usize) -> TabId {
        state.tabs[index].id
    }

    /// Types `text` into the active buffer, the way the code editor would.
    fn edit_active(state: &mut EditorState, text: &str) {
        if let DocumentContent::Text { text: buffer, .. } =
            &mut state.tabs[state.active_tab].content
        {
            *buffer = text.to_string();
        }
    }

    #[test]
    fn dirty_tabs_survive_deletion_so_the_buffer_can_be_saved_back() {
        let path = PathBuf::from("/proj/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"int a;\n"));
        let mut state = state_over(fs.clone());

        open_tab(&mut state, &path);
        assert_eq!(state.tabs.len(), 1);

        // Deleted on disk while still clean → the tab goes away.
        fs.files.borrow_mut().remove(&path);
        prune_missing(&mut state);
        assert!(state.tabs.is_empty());

        // Re-open, type into it, then delete the file again.
        fs.write_file(&path, b"int a;\n").unwrap();
        open_tab(&mut state, &path);
        edit_active(&mut state, "int a; // edited\n");
        assert!(state.tabs[0].unsaved());

        fs.files.borrow_mut().remove(&path);
        prune_missing(&mut state);
        assert_eq!(state.tabs.len(), 1, "a dirty buffer must not be discarded");

        // And saving it recreates the file.
        assert!(save_tab(&mut state, 0));
        assert_eq!(fs.read_file(&path).unwrap(), b"int a; // edited\n".to_vec());
        assert!(!state.tabs[0].unsaved());
    }

    #[test]
    fn renaming_a_parent_folder_keeps_the_unsaved_buffer() {
        let path = PathBuf::from("/proj/src/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"original\n"));
        let mut state = state_over(fs);

        open_tab(&mut state, &path);
        edit_active(&mut state, "typed but not saved\n");

        retarget_after_rename(
            &mut state,
            Path::new("/proj/src"),
            Path::new("/proj/source"),
        );

        let tab = &state.tabs[0];
        assert_eq!(tab.path, PathBuf::from("/proj/source/main.cpp"));
        assert_eq!(tab.name, "main.cpp");
        assert_eq!(
            tab.content.text(),
            Some("typed but not saved\n"),
            "the rename must not re-read the file over the user's edits"
        );
        assert!(tab.unsaved());
    }

    #[test]
    fn external_changes_reload_clean_buffers_and_flag_dirty_ones() {
        let clean = PathBuf::from("/proj/clean.cpp");
        let dirty = PathBuf::from("/proj/dirty.cpp");
        let fs = Rc::new(FakeFs::default());
        fs.write_file(&clean, b"one\n").unwrap();
        fs.write_file(&dirty, b"one\n").unwrap();
        let mut state = state_over(fs.clone());

        open_tab(&mut state, &clean);
        open_tab(&mut state, &dirty);
        state.active_tab = 1;
        edit_active(&mut state, "my unsaved work\n");

        // Something outside the app rewrites both files.
        fs.write_file(&clean, b"two\n").unwrap();
        fs.write_file(&dirty, b"two\n").unwrap();
        sync_open_buffers(&mut state, &[clean.clone(), dirty.clone()]);

        assert_eq!(state.tabs[0].content.text(), Some("two\n"));
        assert!(!state.tabs[0].external_change);
        assert!(
            state.tabs[0].history_reset,
            "a replaced buffer must drop its undo history, or ⌘Z rewinds past the reload"
        );

        assert_eq!(
            state.tabs[1].content.text(),
            Some("my unsaved work\n"),
            "a dirty buffer must never be overwritten by the watcher"
        );
        assert!(state.tabs[1].external_change);
        assert!(
            !state.tabs[1].history_reset,
            "nothing was replaced, so this buffer keeps its history"
        );
    }

    #[test]
    fn a_tab_that_goes_away_hands_back_its_id_so_its_editor_state_can_be_dropped() {
        let path = PathBuf::from("/proj/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
        let mut state = state_over(fs.clone());

        open_tab(&mut state, &path);
        let closed = tab_id(&state, 0);
        request_close_tab(&mut state, closed);

        assert_eq!(take_discarded_tabs(&mut state), vec![closed]);
        assert!(
            take_discarded_tabs(&mut state).is_empty(),
            "draining twice must not act on the same id twice"
        );

        // A tab the watcher prunes counts too — it is just as gone.
        open_tab(&mut state, &path);
        let pruned = tab_id(&state, 0);
        assert_ne!(pruned, closed, "ids are never reused");
        fs.files.borrow_mut().remove(&path);
        prune_missing(&mut state);

        assert!(state.tabs.is_empty());
        assert_eq!(take_discarded_tabs(&mut state), vec![pruned]);
    }

    #[test]
    fn our_own_save_is_not_mistaken_for_an_external_change() {
        let path = PathBuf::from("/proj/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
        let mut state = state_over(fs);

        open_tab(&mut state, &path);
        edit_active(&mut state, "two\n");
        assert!(save_tab(&mut state, 0));

        // The watcher now reports the write we just made.
        sync_open_buffers(&mut state, &[path]);
        assert!(!state.tabs[0].external_change);
        assert!(!state.tabs[0].unsaved());
    }

    #[test]
    fn binary_files_open_read_only_and_are_never_saved() {
        let path = PathBuf::from("/proj/build/libx.dylib");
        let fs = Rc::new(FakeFs::with_file(&path, &[0x00, 0xFF]));
        let mut state = state_over(fs.clone());

        open_tab(&mut state, &path);
        assert_eq!(state.tabs[0].content, DocumentContent::Binary);
        assert!(!state.tabs[0].unsaved());

        // Saving is a no-op, so the file on disk is left untouched.
        assert!(!save_tab(&mut state, 0));
        assert_eq!(fs.read_file(&path).unwrap(), vec![0x00, 0xFF]);
    }

    #[test]
    fn closing_a_dirty_tab_asks_before_discarding() {
        let path = PathBuf::from("/proj/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
        let mut state = state_over(fs.clone());

        open_tab(&mut state, &path);
        let clean = tab_id(&state, 0);
        request_close_tab(&mut state, clean);
        assert!(state.tabs.is_empty(), "a clean tab closes immediately");

        open_tab(&mut state, &path);
        edit_active(&mut state, "two\n");
        let dirty = tab_id(&state, 0);
        request_close_tab(&mut state, dirty);
        assert_eq!(state.tabs.len(), 1, "a dirty tab waits for confirmation");
        assert!(state.explorer.pending_confirm.is_some());

        // "Save" writes the buffer and then closes.
        run_pending_confirm(&mut state, ConfirmChoice::Alternate);
        assert!(state.tabs.is_empty());
        assert_eq!(fs.read_file(&path).unwrap(), b"two\n".to_vec());
    }

    #[test]
    fn quitting_is_guarded_only_while_work_is_unsaved() {
        let path = PathBuf::from("/proj/main.cpp");
        let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
        let mut state = state_over(fs.clone());

        open_tab(&mut state, &path);
        assert!(
            !request_quit(&mut state),
            "nothing dirty → quit straight away"
        );

        edit_active(&mut state, "two\n");
        assert!(request_quit(&mut state));
        assert!(state.explorer.pending_confirm.is_some());

        // "Save all & quit" persists first, then approves the quit.
        assert!(run_pending_confirm(&mut state, ConfirmChoice::Alternate));
        assert_eq!(fs.read_file(&path).unwrap(), b"two\n".to_vec());
        assert!(!has_unsaved_work(&state));
    }

    /// Opens `names` under `/proj` as tabs, each holding its own name as content
    /// so a buffer can be told apart from its neighbours after a reorder.
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

    #[test]
    fn move_tab_reorders_and_keeps_the_active_buffer() {
        let (_fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp", "c.cpp"]);
        state.active_tab = 1;
        let active = tab_id(&state, 1);

        // Drag "a.cpp" into the gap before "c.cpp".
        move_tab(&mut state, 0, 2);
        assert_eq!(tab_names(&state), ["b.cpp", "a.cpp", "c.cpp"]);

        // The active buffer followed its tab rather than staying in slot 1.
        assert_eq!(state.tabs[state.active_tab].id, active);
        assert_eq!(state.tabs[state.active_tab].name, "b.cpp");
        assert_eq!(state.tabs[state.active_tab].content.text(), Some("b.cpp"));

        // Dropping past the end parks a tab last.
        move_tab(&mut state, 0, 3);
        assert_eq!(tab_names(&state), ["a.cpp", "c.cpp", "b.cpp"]);
        assert_eq!(state.tabs[state.active_tab].id, active);
    }

    #[test]
    fn move_tab_ignores_out_of_range_and_no_op_moves() {
        let (_fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp", "c.cpp"]);

        for (from, insert_before) in [(0, 0), (1, 1), (1, 2), (3, 0), (0, 4), (9, 9)] {
            move_tab(&mut state, from, insert_before);
            assert_eq!(
                tab_names(&state),
                ["a.cpp", "b.cpp", "c.cpp"],
                "move_tab({from}, {insert_before}) should have been a no-op"
            );
        }
    }

    #[test]
    fn close_confirm_targets_the_tab_by_identity_not_slot() {
        let (fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp", "c.cpp"]);

        // Dirty "c.cpp" and ask to close it; the confirmation is now pending.
        state.active_tab = 2;
        edit_active(&mut state, "edited c\n");
        let doomed = tab_id(&state, 2);
        request_close_tab(&mut state, doomed);
        assert!(state.explorer.pending_confirm.is_some());

        // While the modal is up the strip moves under it: the user reorders, and
        // the watcher prunes a clean tab whose file was deleted outside the app.
        move_tab(&mut state, 2, 0);
        fs.files.borrow_mut().remove(Path::new("/proj/a.cpp"));
        prune_missing(&mut state);
        assert_eq!(tab_names(&state), ["c.cpp", "b.cpp"]);

        // Discarding must still close "c.cpp" — a stored index would have taken
        // "b.cpp" (or nothing) with it.
        run_pending_confirm(&mut state, ConfirmChoice::Primary);
        assert_eq!(tab_names(&state), ["b.cpp"]);
        assert_eq!(
            fs.read_file(Path::new("/proj/c.cpp")).unwrap(),
            b"c.cpp".to_vec(),
            "discarding must not have written the buffer"
        );
    }

    #[test]
    fn close_confirm_no_ops_when_its_tab_is_already_gone() {
        let (fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp"]);

        state.active_tab = 0;
        edit_active(&mut state, "edited a\n");
        let doomed = tab_id(&state, 0);
        request_close_tab(&mut state, doomed);

        // The buffer is saved and the tab closed by some other route before the
        // user answers, so the pending action's target no longer exists.
        save_tab(&mut state, 0);
        close_tab(&mut state, 0);
        assert_eq!(tab_names(&state), ["b.cpp"]);

        run_pending_confirm(&mut state, ConfirmChoice::Primary);
        assert_eq!(
            tab_names(&state),
            ["b.cpp"],
            "a confirmation whose tab is gone must not close a bystander"
        );
        assert_eq!(
            fs.read_file(Path::new("/proj/a.cpp")).unwrap(),
            b"edited a\n"
        );
    }

    #[test]
    fn unsaved_paths_tracks_the_dirty_set() {
        let (_fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp"]);
        assert!(unsaved_paths(&state).is_empty());

        state.active_tab = 1;
        edit_active(&mut state, "edited b\n");
        assert_eq!(
            unsaved_paths(&state),
            HashSet::from([PathBuf::from("/proj/b.cpp")])
        );

        // The set is keyed on the path, so it survives a reorder unchanged…
        move_tab(&mut state, 1, 0);
        assert_eq!(
            unsaved_paths(&state),
            HashSet::from([PathBuf::from("/proj/b.cpp")])
        );
        // …and empties again on save.
        assert!(save_tab(&mut state, 0));
        assert!(unsaved_paths(&state).is_empty());
    }

    #[test]
    fn saving_a_background_tab_leaves_the_active_tab_alone() {
        let (fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp"]);

        // Dirty the background tab, then make the other one active.
        state.active_tab = 0;
        edit_active(&mut state, "edited a\n");
        state.active_tab = 1;

        let background = tab_id(&state, 0);
        handle_events(
            &mut state,
            EditorViewEvents {
                tab_saved: Some(background),
                ..Default::default()
            },
        );

        assert_eq!(
            fs.read_file(Path::new("/proj/a.cpp")).unwrap(),
            b"edited a\n"
        );
        assert!(!state.tabs[0].unsaved());
        assert_eq!(
            state.active_tab, 1,
            "pressing a background tab's ● must not steal the code pane"
        );
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
        let fs = FakeFs {
            listing,
            ..Default::default()
        };

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
