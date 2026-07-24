//! State and event handling for the Editor window (the IDE shell).
//!
//! The explorer tree is now **real**: it reads the opened project from disk on
//! demand through `ProjectFileSystemPort`, one directory level per folder
//! expansion. Tabs and the terminal remain static mocks from HL9 (real
//! editor/terminal logic arrives in later tasks).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::application::ports::{DirEntryInfo, ProjectFileSystemPort};
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

/// One (mocked) open buffer shown in the tab strip.
pub struct EditorTab {
    pub name: String,
    pub icon: NodeIcon,
    pub unsaved: bool,
}

impl EditorTab {
    fn new(name: &str, unsaved: bool) -> Self {
        Self {
            name: name.to_string(),
            icon: icon_for_file(name),
            unsaved,
        }
    }
}

/// Per-window state for the Editor. The tree is live; tabs/terminal are mocks.
pub struct EditorState {
    pub project_name: String,
    pub project_path: PathBuf,
    pub fs: Rc<dyn ProjectFileSystemPort>,
    pub tree: FileTreeState,
    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,
    pub terminal_lines: Vec<String>,
    pub focus_requested: bool,
}

impl EditorState {
    /// Builds the editor state for a freshly opened project. The explorer tree
    /// is read from disk via `fs`; tabs and terminal remain static mocks.
    pub fn new(project: &RecentProject, fs: Rc<dyn ProjectFileSystemPort>) -> Self {
        let tree = FileTreeState::new(project, fs.as_ref());

        let tabs = vec![
            EditorTab::new("effect_processor.cpp", false),
            EditorTab::new("effect_processor.h", true),
        ];

        let terminal_lines = vec![
            format!("{} $ make dylib", project.name),
            "  (build output will appear here)".to_string(),
        ];

        Self {
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            fs,
            tree,
            tabs,
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
    /// A file the user clicked (selection highlight).
    pub select: Option<PathBuf>,
}

/// What the user did in the Editor view this frame.
#[derive(Default)]
pub struct EditorViewEvents {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    pub tab_clicked: Option<usize>,
    pub explorer_expand: Vec<PathBuf>,
    pub explorer_select: Option<PathBuf>,
}

/// Follow-up actions the app must perform after handling the frame's events.
#[derive(Default)]
pub struct EditorRequests {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
}

/// Applies view events to the state and returns the actions the app must take.
pub fn handle_events(state: &mut EditorState, events: EditorViewEvents) -> EditorRequests {
    if let Some(index) = events.tab_clicked
        && index < state.tabs.len()
    {
        state.active_tab = index;
    }

    // Lazily read the children of any folder opened this frame.
    let fs = state.fs.clone();
    for path in events.explorer_expand {
        if let Some(node) = find_dir_mut(&mut state.tree.root, &path)
            && !node.loaded
        {
            node.children = read_children(fs.as_ref(), &node.path);
            node.loaded = true;
        }
    }

    if let Some(selected) = events.explorer_select {
        state.tree.selected = Some(selected);
    }

    EditorRequests {
        open_emulator: events.open_emulator,
        build_run: events.build_run,
        compile: events.compile,
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
}
