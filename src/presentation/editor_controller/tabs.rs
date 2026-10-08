//! Open editor tabs: loading, saving, closing, reordering and keeping buffers
//! in step with the files on disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::EditorState;
use super::confirm::{EditorAction, PendingConfirm};
use super::file_tree::{NodeIcon, icon_for_file};
use crate::application::file_system_service::FileSystemService;
use crate::domain::file_system::FileSystemError;
use crate::domain::text_document::{DocumentContent, Language, language_for};
use crate::presentation::components::molecules::find_bar::FindState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

pub struct EditorTab {
    pub id: TabId,
    pub name: String,
    pub icon: NodeIcon,
    pub path: PathBuf,
    pub language: Language,
    pub content: DocumentContent,
    pub saved_text: String,
    pub disk_modified: Option<SystemTime>,
    pub external_change: bool,
    pub history_reset: bool,
    pub find: Option<FindState>,
    pub pending_select: Option<std::ops::Range<usize>>,
}

impl EditorTab {
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
            pending_select: None,
        }
    }

    pub fn unsaved(&self) -> bool {
        match self.content.text() {
            Some(text) => text != self.saved_text,
            None => false,
        }
    }

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

impl EditorState {
    fn take_tab_id(&mut self) -> TabId {
        self.next_tab_id += 1;
        TabId(self.next_tab_id)
    }
}

pub fn jump_to_diagnostic(state: &mut EditorState, file: &str, line: u32, column: Option<u32>) {
    let candidate = Path::new(file);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        state.project_path.join(candidate)
    };

    let path = if state.fs.exists(&resolved) {
        resolved
    } else {
        let name = candidate.file_name();
        match state
            .tabs
            .iter()
            .find(|tab| name.is_some_and(|n| tab.path.file_name() == Some(n)))
        {
            Some(tab) => tab.path.clone(),
            None => {
                state.explorer.error = Some(format!("Could not find {file}"));
                return;
            }
        }
    };

    open_tab(state, &path);

    if let Some(tab) = state.tabs.get_mut(state.active_tab)
        && let Some(text) = tab.content.text()
        && let Some(span) = crate::domain::diagnostics::span_in(text, line, column)
    {
        tab.pending_select = Some(span);
    }
}

pub fn take_discarded_tabs(state: &mut EditorState) -> Vec<TabId> {
    std::mem::take(&mut state.discarded_tabs)
}

pub fn unsaved_paths(state: &EditorState) -> HashSet<PathBuf> {
    state
        .tabs
        .iter()
        .filter(|t| t.unsaved())
        .map(|t| t.path.clone())
        .collect()
}

pub(super) fn sync_open_buffers(state: &mut EditorState, changed: &[PathBuf]) {
    let fs_service = state.fs_service.clone();

    for tab in &mut state.tabs {
        if !changed.iter().any(|p| p == &tab.path) {
            continue;
        }
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

pub(super) fn save_active(state: &mut EditorState) {
    save_tab(state, state.active_tab);
}

pub(super) fn save_all(state: &mut EditorState) {
    for index in 0..state.tabs.len() {
        if state.tabs[index].unsaved() {
            save_tab(state, index);
        }
    }
}

pub(super) fn save_tab(state: &mut EditorState, index: usize) -> bool {
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

pub(super) fn reload_active(state: &mut EditorState) {
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

pub fn open_find(state: &mut EditorState) {
    if let Some(tab) = state.tabs.get_mut(state.active_tab)
        && tab.content.is_editable()
    {
        tab.find = Some(FindState::default());
    }
}

pub fn save_active_tab(state: &mut EditorState) {
    save_active(state);
}

pub fn save_all_tabs(state: &mut EditorState) {
    save_all(state);
}

pub fn close_active_tab(state: &mut EditorState) {
    if let Some(tab) = state.tabs.get(state.active_tab) {
        request_close_tab(state, tab.id);
    }
}

pub(super) fn open_tab(state: &mut EditorState, path: &Path) {
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

pub(super) fn request_close_tab(state: &mut EditorState, id: TabId) {
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

pub fn move_tab(state: &mut EditorState, from: usize, insert_before: usize) {
    let len = state.tabs.len();
    if from >= len || insert_before > len {
        return;
    }
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

pub(super) fn close_tab(state: &mut EditorState, index: usize) {
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

pub(super) fn retarget_after_rename(state: &mut EditorState, old: &Path, new: &Path) {
    if let Some(sel) = state.tree.selected.clone() {
        if sel == old {
            state.tree.selected = Some(new.to_path_buf());
        } else if let Ok(rest) = sel.strip_prefix(old) {
            state.tree.selected = Some(new.join(rest));
        }
    }
    for tab in &mut state.tabs {
        if tab.path == old {
            tab.retarget(new);
        } else if let Ok(rest) = tab.path.strip_prefix(old) {
            let moved = new.join(rest);
            tab.retarget(&moved);
        }
    }
}

pub(super) fn prune_missing(state: &mut EditorState) {
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
