//! The Editor window: the VS Code-style IDE shell. Composes the toolbar,
//! explorer, tab strip, code area, terminal and status bar as nested panels.
//! Every panel is live: the terminal runs real PTY sessions, and the toolbar's
//! build buttons feed their `make` target into its Build tab.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::presentation::components::molecules::confirm_modal::{
    ConfirmModalContent, confirm_modal,
};
use crate::presentation::components::molecules::editor_tab::{EditorTabRequest, editor_tab};
use crate::presentation::components::molecules::error_dialog::error_dialog;
use crate::presentation::components::organisms::code_pane::code_pane;
use crate::presentation::components::organisms::editor_toolbar::editor_toolbar;
use crate::presentation::components::organisms::file_explorer::file_explorer;
use crate::presentation::components::organisms::problems_strip;
use crate::presentation::components::organisms::status_bar::{StatusInfo, status_bar};
use crate::presentation::components::organisms::terminal_panel;
use crate::presentation::editor_controller::{self, EditorState, EditorViewEvents};

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
    //
    // While the terminal has focus it gets every key it can use, so ⌘F reaches
    // a shell program that wants it. Save is the exception: "save my work" must
    // not depend on where the caret happens to be, so it stays global.
    let terminal_focused = terminal_panel::has_focus(ui.ctx());
    ui.input_mut(|input| {
        events.code.save_all = input.consume_shortcut(&SAVE_ALL);
        events.code.save = !events.code.save_all && input.consume_shortcut(&SAVE);
        if !terminal_focused {
            events.code.open_find = input.consume_shortcut(&FIND);
        }
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
        let bar = status_bar(
            ui,
            StatusInfo {
                project_name: &state.project_name,
                tab: state.tabs.get(state.active_tab),
                cursor: state.cursor,
                build: state.terminal.build_status(),
                diagnostics: state.terminal.diagnostic_counts(),
            },
        );
        events.code.save |= bar.save;
    });

    // Which open buffers are dirty, for the explorer's ● markers. Computed once
    // here so the panel closure doesn't have to borrow the tab list.
    let unsaved = editor_controller::unsaved_paths(state);

    // Explorer (full-height, left, between toolbar and status bar). Handles
    // open/select, inline create/rename, delete requests and reveals.
    egui::Panel::left("explorer")
        .resizable(true)
        .default_size(220.0)
        .show(ui, |ui| {
            events.explorer = file_explorer(ui, &state.tree, &mut state.explorer, &unsaved);
        });

    // Terminal (bottom, above the status bar, right of the explorer).
    egui::Panel::bottom("terminal")
        .resizable(true)
        .default_size(160.0)
        .show(ui, |ui| {
            let requests = terminal_panel::terminal_panel(ui, &mut state.terminal);
            // Clipboard writes and the error banner are the window's to serve —
            // the controller stays free of egui.
            if let Some(text) = requests.copy {
                ui.ctx().copy_text(text);
            }
            if let Some(error) = requests.error {
                state.explorer.error = Some(error);
            }
            // A successful `make dylib` opens the simulator on what it just
            // built, through the same path the toolbar's own button uses.
            events.open_emulator |= requests.launch_simulator;
        });

    // Problems strip (between the code area and the terminal). Registered after
    // the terminal panel so it sits above it, and it draws nothing at all when
    // the last build was clean.
    if !state.terminal.diagnostics.is_empty() {
        let diagnostics = std::mem::take(&mut state.terminal.diagnostics);
        let stale: Vec<PathBuf> = state.terminal.stale_files.iter().cloned().collect();
        let root = state.project_path.clone();

        let mut jump = None;
        egui::Panel::bottom("problems")
            .resizable(true)
            .default_size(76.0)
            .show(ui, |ui| {
                jump = problems_strip::problems_strip(ui, &diagnostics, &|file| {
                    is_stale(&root, &stale, file)
                });
            });

        state.terminal.diagnostics = diagnostics;
        if let Some(jump) = jump {
            editor_controller::jump_to_diagnostic(state, &jump.file, jump.line, jump.column);
        }
    }

    // Editor: tab strip on top, code area filling the rest.
    let any_unsaved = !unsaved.is_empty();
    // Consumed here so a single request scrolls once, not on every later frame.
    let scroll_active = std::mem::take(&mut state.scroll_active_into_view);

    egui::CentralPanel::default().show(ui, |ui| {
        egui::Panel::top("tabs").show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    // A drop is reported by whichever tab the pointer was over,
                    // which also tells us the landing slot.
                    let mut dropped: Option<(_, usize)> = None;

                    for (index, tab) in state.tabs.iter().enumerate() {
                        let response = editor_tab(
                            ui,
                            EditorTabRequest {
                                tab,
                                index,
                                active: index == state.active_tab,
                                any_unsaved,
                                scroll_to: scroll_active && index == state.active_tab,
                            },
                        );

                        if let (Some(dragged), Some(slot)) =
                            (response.dropped, response.drop_before)
                        {
                            dropped = Some((dragged, slot));
                        }
                        if response.close_clicked {
                            events.tab_closed = Some(tab.id);
                        } else if response.clicked {
                            events.tab_clicked = Some(index);
                        }
                        if response.save_clicked {
                            events.tab_saved = Some(tab.id);
                        }
                        if response.save_all_clicked {
                            events.code.save_all = true;
                        }
                        if response.reveal_clicked {
                            events.tab_reveal = Some(tab.path.clone());
                        }
                        // Clipboard access is egui's, so it is served here rather
                        // than round-tripped through the egui-free controller.
                        if response.copy_path_clicked {
                            ui.ctx().copy_text(tab.path.to_string_lossy().into_owned());
                        }
                    }

                    events.tab_reordered = dropped;
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let active = state.active_tab;
            // Cloned out before the tabs are borrowed mutably; the code editor's
            // Paste menu item needs it, and egui has no clipboard read of its own.
            let clipboard = state.clipboard.clone();

            // Diagnostics for the buffer about to be drawn, and whether it has
            // been edited since the build that produced them. Both are resolved
            // here, before `tabs` is borrowed mutably.
            let active_path = state.tabs.get(active).map(|tab| tab.path.clone());
            let (mine, stale) = match &active_path {
                Some(path) => (
                    state
                        .terminal
                        .diagnostics
                        .iter()
                        .filter(|d| {
                            d.file.as_deref().is_some_and(|file| {
                                names_same_file(&state.project_path, path, file)
                            })
                        })
                        .cloned()
                        .collect::<Vec<_>>(),
                    state.terminal.is_stale(path),
                ),
                None => (Vec::new(), false),
            };

            let pane = code_pane(
                ui,
                state.tabs.get_mut(active),
                clipboard.as_ref(),
                &mine,
                stale,
            );
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

/// Whether the file a diagnostic names has been edited since the build.
///
/// The compiler's path and the editor's path rarely match verbatim — one is
/// relative to `make`'s working directory, the other absolute — so this compares
/// the resolved path first and falls back to the file name.
fn is_stale(root: &Path, stale: &[PathBuf], file: &str) -> bool {
    let candidate = Path::new(file);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    stale.iter().any(|path| {
        *path == resolved
            || (candidate.file_name().is_some() && path.file_name() == candidate.file_name())
    })
}

/// Whether the path a compiler wrote refers to the buffer at `path`.
///
/// Resolved against the project root first — `make` runs there, so most paths
/// are relative to it — and by file name as a fallback, which covers the forms
/// that do not survive a shell (git-bash's `/c/...` on Windows).
fn names_same_file(root: &Path, path: &Path, file: &str) -> bool {
    let candidate = Path::new(file);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    path == resolved
        || (candidate.file_name().is_some() && path.file_name() == candidate.file_name())
}
