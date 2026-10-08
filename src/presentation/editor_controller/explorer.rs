//! The file explorer's interactions: expanding folders, inline create/rename,
//! delete requests and revealing entries in the OS file manager.

use std::path::{Path, PathBuf};

use super::EditorState;
use super::confirm::{EditorAction, PendingConfirm};
use super::events::ExplorerEvents;
use super::file_tree::{NodeIcon, find_dir_mut, find_node, read_children, refresh_dir};
use super::tabs::{open_tab, retarget_after_rename};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

pub fn entry_kind_icon(kind: EntryKind) -> NodeIcon {
    match kind {
        EntryKind::File => NodeIcon::Generic,
        EntryKind::Directory => NodeIcon::FolderClosed,
    }
}

pub struct PendingCreate {
    pub parent: PathBuf,
    pub kind: EntryKind,
    pub buffer: String,
    pub focus: bool,
}

pub struct PendingRename {
    pub path: PathBuf,
    pub buffer: String,
    pub focus: bool,
}

#[derive(Default)]
pub struct ExplorerUiState {
    pub pending_create: Option<PendingCreate>,
    pub pending_rename: Option<PendingRename>,
    pub pending_confirm: Option<PendingConfirm>,
    pub error: Option<String>,
}

pub fn begin_create_at_selection(state: &mut EditorState, kind: EntryKind) {
    let root = state.tree.root.path.clone();
    let parent = match state.tree.selected.as_deref() {
        Some(path) if find_dir_mut(&mut state.tree.root, path).is_some() => path.to_path_buf(),
        Some(path) => path.parent().map(Path::to_path_buf).unwrap_or(root),
        None => root,
    };
    state.explorer.pending_rename = None;
    state.explorer.pending_create = Some(PendingCreate {
        parent,
        kind,
        buffer: String::new(),
        focus: true,
    });
}

pub(super) fn handle_explorer(state: &mut EditorState, events: ExplorerEvents) -> bool {
    let mut open_doom = false;

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
        if crate::domain::doom::is_doom_file(&path) {
            open_doom = true;
        } else {
            open_tab(state, &path);
        }
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

    open_doom
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

pub(super) fn refresh_parent(state: &mut EditorState, dir: &Path) {
    let fs = state.fs.clone();
    if let Some(node) = find_dir_mut(&mut state.tree.root, dir)
        && node.loaded
    {
        refresh_dir(node, fs.as_ref());
    }
}
