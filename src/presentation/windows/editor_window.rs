use std::path::{Path, PathBuf};

use eframe::egui;

use crate::domain::menu;
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
use crate::presentation::menu_controller;

pub fn show(ui: &mut egui::Ui, state: &mut EditorState) -> EditorViewEvents {
    let mut events = EditorViewEvents::default();

    let save = menu_controller::shortcut(menu::SAVE);
    let save_all = menu_controller::shortcut(menu::SAVE_ALL);
    let find = menu_controller::shortcut(menu::FIND);
    let close_tab = menu_controller::shortcut(menu::CLOSE_TAB);
    let close_window = menu_controller::shortcut(menu::CLOSE_WINDOW);
    let quit = menu_controller::shortcut(menu::QUIT);

    let terminal_focused = terminal_panel::has_focus(ui.ctx());
    ui.input_mut(|input| {
        events.code.save_all = input.consume_shortcut(&save_all);
        events.code.save = !events.code.save_all && input.consume_shortcut(&save);
        let window = input.consume_shortcut(&close_window);
        events.quit_requested = window | input.consume_shortcut(&quit);
        events.close_active_tab = !window && input.consume_shortcut(&close_tab);
        if !terminal_focused {
            events.code.open_find = input.consume_shortcut(&find);
        }
    });

    let injected = editor_controller::take_pending_command(state);
    if editor_controller::has_pending_commands(state) {
        ui.ctx().request_repaint();
    }

    egui::Panel::top("editor_toolbar").show(ui, |ui| {
        let toolbar = editor_toolbar(ui);
        events.open_emulator = toolbar.open_emulator;
        events.build_run = toolbar.build_run;
        events.compile = toolbar.compile;
    });

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

    let unsaved = editor_controller::unsaved_paths(state);

    egui::Panel::left("explorer")
        .resizable(true)
        .default_size(220.0)
        .show(ui, |ui| {
            events.explorer = file_explorer(ui, &state.tree, &mut state.explorer, &unsaved);
        });

    egui::Panel::bottom("terminal")
        .resizable(true)
        .default_size(160.0)
        .show(ui, |ui| {
            let requests = terminal_panel::terminal_panel(ui, &mut state.terminal);
            if let Some(text) = requests.copy {
                ui.ctx().copy_text(text);
            }
            if let Some(error) = requests.error {
                state.explorer.error = Some(error);
            }
            events.reload_plugin |= requests.reload_plugin;
            events.build_failed |= requests.build_failed;
        });

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

    let any_unsaved = !unsaved.is_empty();
    let scroll_active = std::mem::take(&mut state.scroll_active_into_view);

    egui::CentralPanel::default().show(ui, |ui| {
        egui::Panel::top("tabs").show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
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
            let clipboard = state.clipboard.clone();

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
                injected,
            );
            events.code.edited = pane.edited;
            events.code.reload = pane.reload;
            events.code.close_find |= pane.close_find;
            if pane.cursor.is_some() {
                state.cursor = pane.cursor;
            }
        });
    });

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
