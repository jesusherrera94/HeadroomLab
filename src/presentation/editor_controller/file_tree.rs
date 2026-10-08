use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::application::ports::{DirEntryInfo, ProjectFileSystemPort};
use crate::domain::project::RecentProject;

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

pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
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

pub struct FileTreeState {
    pub root: TreeNode,
    pub selected: Option<PathBuf>,
}

impl FileTreeState {
    pub(super) fn new(project: &RecentProject, fs: &dyn ProjectFileSystemPort) -> Self {
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

pub(super) fn read_children(fs: &dyn ProjectFileSystemPort, dir: &Path) -> Vec<TreeNode> {
    let mut nodes: Vec<TreeNode> = fs
        .read_dir(dir)
        .into_iter()
        .map(TreeNode::from_entry)
        .collect();
    nodes.sort_by(cmp_nodes);
    nodes
}

pub(super) fn cmp_nodes(a: &TreeNode, b: &TreeNode) -> Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| natural_cmp(&a.name, &b.name))
}

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

fn cmp_numeric(a: &str, b: &str) -> Ordering {
    let ta = a.trim_start_matches('0');
    let tb = b.trim_start_matches('0');
    ta.len()
        .cmp(&tb.len())
        .then_with(|| ta.cmp(tb))
        .then_with(|| a.len().cmp(&b.len()))
}

pub(super) fn find_dir_mut<'a>(node: &'a mut TreeNode, path: &Path) -> Option<&'a mut TreeNode> {
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

pub(super) fn find_node<'a>(node: &'a TreeNode, path: &Path) -> Option<&'a TreeNode> {
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

pub(super) fn refresh_dir(node: &mut TreeNode, fs: &dyn ProjectFileSystemPort) {
    let fresh = read_children(fs, &node.path);
    let mut old: HashMap<PathBuf, TreeNode> = node
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

pub fn row_icon(node: &TreeNode, open: Option<bool>) -> NodeIcon {
    match open {
        Some(true) => NodeIcon::FolderOpen,
        Some(false) => NodeIcon::FolderClosed,
        None => node.icon,
    }
}
