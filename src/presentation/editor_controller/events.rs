//! The per-frame event bundle the editor window produces, and the requests the
//! controller hands back to the app after applying it.

use std::path::PathBuf;

use super::EditorState;
use super::confirm::{ConfirmChoice, ConfirmOutcome, run_pending_confirm};
use super::explorer::{EntryKind, handle_explorer};
use super::tabs::{
    TabId, close_active_tab, move_tab, reload_active, request_close_tab, save_active, save_all,
    save_tab,
};
use crate::domain::project::RecentProject;
use crate::presentation::components::molecules::find_bar::FindState;

#[derive(Default)]
pub struct ExplorerEvents {
    pub expand: Vec<PathBuf>,
    pub open: Option<PathBuf>,
    pub select: Option<PathBuf>,
    pub begin_create: Option<(PathBuf, EntryKind)>,
    pub commit_create: bool,
    pub cancel_create: bool,
    pub begin_rename: Option<PathBuf>,
    pub commit_rename: bool,
    pub cancel_rename: bool,
    pub request_delete: Option<PathBuf>,
    pub reveal: Option<PathBuf>,
}

#[derive(Default)]
pub struct CodeEvents {
    pub edited: bool,
    pub cursor: Option<(usize, usize)>,
    pub save: bool,
    pub save_all: bool,
    pub reload: bool,
    pub open_find: bool,
    pub close_find: bool,
}

#[derive(Default)]
pub struct EditorViewEvents {
    pub open_emulator: bool,
    pub reload_plugin: bool,
    pub build_failed: bool,
    pub build_run: bool,
    pub compile: bool,
    pub tab_clicked: Option<usize>,
    pub tab_closed: Option<TabId>,
    pub tab_saved: Option<TabId>,
    pub tab_reordered: Option<(TabId, usize)>,
    pub tab_reveal: Option<PathBuf>,
    pub explorer: ExplorerEvents,
    pub code: CodeEvents,
    pub confirm_confirmed: bool,
    pub confirm_alternate: bool,
    pub confirm_cancelled: bool,
    pub error_dismissed: bool,
    pub close_active_tab: bool,
    pub quit_requested: bool,
}

#[derive(Default)]
pub struct EditorRequests {
    pub open_emulator: bool,
    pub open_doom: bool,
    pub reload_plugin: bool,
    pub build_failed: bool,
    pub build_run: bool,
    pub compile: bool,
    pub quit_confirmed: bool,
    pub quit_requested: bool,
    pub switch_project: Option<RecentProject>,
}

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

    if events.close_active_tab {
        close_active_tab(state);
    }

    let open_doom = handle_explorer(state, events.explorer);
    handle_code(state, events.code);

    let mut confirmed = ConfirmOutcome::default();
    if events.confirm_confirmed {
        confirmed = run_pending_confirm(state, ConfirmChoice::Primary);
    }
    if events.confirm_alternate {
        confirmed = run_pending_confirm(state, ConfirmChoice::Alternate);
    }
    if events.confirm_cancelled {
        state.explorer.pending_confirm = None;
    }
    if events.error_dismissed {
        state.explorer.error = None;
    }

    EditorRequests {
        open_emulator: events.open_emulator,
        open_doom,
        reload_plugin: events.reload_plugin,
        build_failed: events.build_failed,
        build_run: events.build_run,
        compile: events.compile,
        quit_confirmed: confirmed.quit,
        quit_requested: events.quit_requested,
        switch_project: confirmed.switch_to,
    }
}

fn handle_code(state: &mut EditorState, events: CodeEvents) {
    if events.edited
        && let Some(tab) = state.tabs.get(state.active_tab)
    {
        state.terminal.stale_files.insert(tab.path.clone());
    }
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
