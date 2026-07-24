//! The Editor window: the VS Code-style IDE shell. Composes the toolbar,
//! explorer, tab strip, code area, terminal and status bar as nested panels.
//! HL9 is layout only — every panel but the toolbar's "Open emulator" button is
//! a static mock built from the opened project.

use eframe::egui::{self, RichText};

use crate::presentation::components::molecules::confirm_modal::{
    ConfirmModalContent, confirm_modal,
};
use crate::presentation::components::molecules::editor_tab::editor_tab;
use crate::presentation::components::molecules::error_dialog::error_dialog;
use crate::presentation::components::organisms::editor_toolbar::editor_toolbar;
use crate::presentation::components::organisms::file_explorer::file_explorer;
use crate::presentation::components::organisms::status_bar::status_bar;
use crate::presentation::components::organisms::terminal_panel::terminal_panel;
use crate::presentation::editor_controller::{EditorState, EditorViewEvents};
use crate::presentation::theme;

pub fn show(ui: &mut egui::Ui, state: &mut EditorState) -> EditorViewEvents {
    let mut events = EditorViewEvents::default();

    // Toolbar (full-width, top).
    egui::Panel::top("editor_toolbar").show(ui, |ui| {
        let toolbar = editor_toolbar(ui);
        events.open_emulator = toolbar.open_emulator;
        events.build_run = toolbar.build_run;
        events.compile = toolbar.compile;
    });

    // Status bar (full-width, very bottom).
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        status_bar(ui, &state.project_name);
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

        egui::CentralPanel::default().show(ui, |ui| {
            code_area(ui, state);
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
                destructive: true,
            },
        );
        events.confirm_confirmed = modal.confirmed;
        events.confirm_cancelled = modal.cancelled;
    } else if let Some(error) = &state.explorer.error
        && error_dialog(ui.ctx(), error)
    {
        events.error_dismissed = true;
    }

    events
}

/// Static placeholder code for the active tab. The real editable, syntax-
/// highlighted view arrives in Day 7.
fn code_area(ui: &mut egui::Ui, state: &EditorState) {
    let Some(tab) = state.tabs.get(state.active_tab) else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select a file to open it")
                    .font(theme::body_font())
                    .color(theme::MUTED_ON_DARK),
            );
        });
        return;
    };
    let title = tab.name.as_str();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(MOCK_CODE.replace("{FILE}", title))
                    .font(egui::FontId::monospace(theme::FONT_BODY))
                    .color(theme::LABEL_ON_DARK),
            );
        });
}

const MOCK_CODE: &str = "\
// {FILE}
#include \"effect_processor.h\"

void EffectProcessor::Process(float* samples, size_t count) {
    for (size_t i = 0; i < count; ++i) {
        samples[i] = ProcessSample(samples[i]);
    }
}

float EffectProcessor::ProcessSample(float in) {
    // TODO: your DSP here
    return in;
}
";
