//! The right-click menu over the code area.

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};

use super::commands::selection_of;
use crate::domain::editing::EditorCommand;
use crate::domain::menu;
use crate::domain::text_document::{Language, line_comment};

pub(super) fn editor_menu(
    ui: &mut egui::Ui,
    text: &str,
    id: egui::Id,
    language: Language,
    can_paste: bool,
) -> Option<EditorCommand> {
    ui.set_min_width(220.0);

    let selection = selection_of(ui, id, text).unwrap_or(0..0);
    let has_selection = !selection.is_empty();

    let state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
    let here = (
        CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end)),
        text.to_owned(),
    );
    let undoer = state.undoer();

    let mut chosen = None;
    let mut item = |ui: &mut egui::Ui, enabled, label, command| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(label)
                    .shortcut_text(menu::chord_for(command).to_string())
                    .frame(false),
            )
            .clicked()
        {
            chosen = Some(command);
            ui.close();
        }
    };

    item(ui, has_selection, "Cut", EditorCommand::Cut);
    item(ui, has_selection, "Copy", EditorCommand::Copy);
    item(ui, can_paste, "Paste", EditorCommand::Paste);
    ui.separator();
    item(ui, undoer.has_undo(&here), "Undo", EditorCommand::Undo);
    item(ui, undoer.has_redo(&here), "Redo", EditorCommand::Redo);
    ui.separator();
    item(ui, true, "Select Line", EditorCommand::SelectLine);
    item(
        ui,
        line_comment(language).is_some(),
        "Toggle Comment",
        EditorCommand::ToggleComment,
    );
    item(ui, true, "Select All", EditorCommand::SelectAll);

    chosen
}
