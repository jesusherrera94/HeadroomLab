//! The Editor window: the VS Code-style IDE shell. Composes the toolbar,
//! explorer, tab strip, code area, terminal and status bar as nested panels.
//! The explorer and the code area are live; the terminal remains a mock until
//! the PTY task.

use eframe::egui;

use crate::presentation::components::molecules::confirm_modal::{
    ConfirmModalContent, confirm_modal,
};
use crate::presentation::components::molecules::editor_tab::editor_tab;
use crate::presentation::components::molecules::error_dialog::error_dialog;
use crate::presentation::components::organisms::code_pane::code_pane;
use crate::presentation::components::organisms::editor_toolbar::editor_toolbar;
use crate::presentation::components::organisms::file_explorer::file_explorer;
use crate::presentation::components::organisms::status_bar::{StatusInfo, status_bar};
use crate::presentation::components::organisms::terminal_panel::terminal_panel;
use crate::presentation::editor_controller::{EditorState, EditorViewEvents};

/// Cmd+S / Ctrl+S — save the active buffer.
const SAVE: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
/// Cmd+Shift+S / Ctrl+Shift+S — save every dirty buffer.
const SAVE_ALL: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::S,
);
/// Cmd+F / Ctrl+F — find in the active buffer.
const FIND: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F);

pub fn show(ui: &mut egui::Ui, state: &mut EditorState) -> EditorViewEvents {
    let mut events = EditorViewEvents::default();

    // Consume the editor shortcuts before any widget sees the keys. Save-all is
    // checked first: it also matches the plain save shortcut's key.
    ui.input_mut(|input| {
        events.code.save_all = input.consume_shortcut(&SAVE_ALL);
        events.code.save = !events.code.save_all && input.consume_shortcut(&SAVE);
        events.code.open_find = input.consume_shortcut(&FIND);
    });

    // Toolbar (full-width, top).
    egui::Panel::top("editor_toolbar").show(ui, |ui| {
        let toolbar = editor_toolbar(ui);
        events.open_emulator = toolbar.open_emulator;
        events.build_run = toolbar.build_run;
        events.compile = toolbar.compile;
    });

    // Status bar (full-width, very bottom). Rendered before the central panel so
    // it reports the cursor from the previous frame — one frame of lag on a
    // position readout is imperceptible and avoids a second layout pass.
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        status_bar(
            ui,
            StatusInfo {
                project_name: &state.project_name,
                tab: state.tabs.get(state.active_tab),
                cursor: state.cursor,
            },
        );
    });

    // Explorer (full-height, left, between toolbar and status bar). Handles
    // open/select, inline create/rename, delete requests and reveals.
    egui::Panel::left("explorer")
        .resizable(true)
        .default_size(220.0)
        .show(ui, |ui| {
            events.explorer = file_explorer(ui, &state.tree, &mut state.explorer);
        });

    // Terminal (bottom, above the status bar, right of the explorer).
    egui::Panel::bottom("terminal")
        .resizable(true)
        .default_size(160.0)
        .show(ui, |ui| {
            terminal_panel(ui, &state.terminal_lines);
        });

    // Editor: tab strip on top, code area filling the rest.
    egui::CentralPanel::default().show(ui, |ui| {
        egui::Panel::top("tabs").show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in state.tabs.iter().enumerate() {
                        let tab = editor_tab(ui, tab, index == state.active_tab);
                        if tab.close_clicked {
                            events.tab_closed = Some(index);
                        } else if tab.clicked {
                            events.tab_clicked = Some(index);
                        }
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let active = state.active_tab;
            let pane = code_pane(ui, state.tabs.get_mut(active));
            events.code.edited = pane.edited;
            events.code.reload = pane.reload;
            events.code.close_find |= pane.close_find;
            // Keep the last known position when focus moves elsewhere, rather
            // than snapping the readout back to Ln 1.
            if pane.cursor.is_some() {
                state.cursor = pane.cursor;
            }
        });
    });

    // Modals over the whole editor: confirmation takes priority over the error
    // banner. Both are reusable, state-driven components.
    if let Some(confirm) = &state.explorer.pending_confirm {
        let modal = confirm_modal(
            ui.ctx(),
            ConfirmModalContent {
                title: &confirm.title,
                message: &confirm.message,
                confirm_label: &confirm.confirm_label,
                alternate_label: confirm.alternate_label.as_deref(),
                destructive: true,
            },
        );
        events.confirm_confirmed = modal.confirmed;
        events.confirm_alternate = modal.alternate;
        events.confirm_cancelled = modal.cancelled;
    } else if let Some(error) = &state.explorer.error
        && error_dialog(ui.ctx(), error)
    {
        events.error_dismissed = true;
    }

    events
}
