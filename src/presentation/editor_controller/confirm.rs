//! Confirmation dialogs: what a pending confirmation will do, and running it
//! once the user picks a button.

use std::path::PathBuf;

use super::EditorState;
use super::explorer::refresh_parent;
use super::tabs::{TabId, close_tab, has_unsaved_work, prune_missing, save_all, save_tab};
use crate::domain::project::RecentProject;

pub struct PendingConfirm {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub alternate_label: Option<String>,
    pub action: EditorAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Primary,
    Alternate,
}

#[derive(Default)]
pub struct ConfirmOutcome {
    pub quit: bool,
    pub switch_to: Option<RecentProject>,
}

pub enum EditorAction {
    Delete(PathBuf),
    CloseTab(TabId),
    Quit,
    SwitchProject(RecentProject),
}

pub fn request_switch_project(state: &mut EditorState, project: RecentProject) -> bool {
    if !has_unsaved_work(state) {
        return false;
    }
    let count = state.tabs.iter().filter(|t| t.unsaved()).count();
    let message = if count == 1 {
        "1 file has unsaved changes. Save before opening another project?".to_owned()
    } else {
        format!("{count} files have unsaved changes. Save before opening another project?")
    };
    state.explorer.pending_confirm = Some(PendingConfirm {
        title: "Unsaved changes".to_string(),
        message,
        confirm_label: "Discard & open".to_string(),
        alternate_label: Some("Save all & open".to_string()),
        action: EditorAction::SwitchProject(project),
    });
    true
}

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

pub(super) fn run_pending_confirm(
    state: &mut EditorState,
    choice: ConfirmChoice,
) -> ConfirmOutcome {
    let Some(confirm) = state.explorer.pending_confirm.take() else {
        return ConfirmOutcome::default();
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
            ConfirmOutcome::default()
        }
        EditorAction::CloseTab(id) => {
            let Some(index) = state.index_of(id) else {
                return ConfirmOutcome::default();
            };
            if choice == ConfirmChoice::Alternate && !save_tab(state, index) {
                return ConfirmOutcome::default();
            }
            close_tab(state, index);
            ConfirmOutcome::default()
        }
        EditorAction::Quit => {
            if choice == ConfirmChoice::Alternate {
                save_all(state);
                if has_unsaved_work(state) {
                    return ConfirmOutcome::default();
                }
            }
            ConfirmOutcome {
                quit: true,
                switch_to: None,
            }
        }
        EditorAction::SwitchProject(project) => {
            if choice == ConfirmChoice::Alternate {
                save_all(state);
                if has_unsaved_work(state) {
                    return ConfirmOutcome::default();
                }
            }
            ConfirmOutcome {
                quit: false,
                switch_to: Some(project),
            }
        }
    }
}
