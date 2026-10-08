//! Running an `EditorCommand` against the buffer, recording undo history so
//! every command can be reverted like a keystroke.

use std::ops::Range;

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};

use crate::application::ports::ClipboardPort;
use crate::domain::editing::{self, Edit, EditorCommand};
use crate::domain::text_document::{Language, line_comment};

pub(super) struct CommandOutcome {
    pub(super) selection: Range<usize>,
    pub(super) scroll: bool,
    pub(super) changed: bool,
}

pub(super) fn run_command(
    ui: &egui::Ui,
    id: egui::Id,
    text: &mut String,
    language: Language,
    clipboard: &dyn ClipboardPort,
    command: EditorCommand,
) -> Option<CommandOutcome> {
    let selection = selection_of(ui, id, text).unwrap_or(0..0);

    match command {
        EditorCommand::SelectAll => Some(CommandOutcome {
            selection: 0..text.chars().count(),
            scroll: false,
            changed: false,
        }),

        EditorCommand::Copy => {
            ui.ctx().copy_text(slice_of(text, selection));
            None
        }

        EditorCommand::Cut => {
            ui.ctx().copy_text(slice_of(text, selection.clone()));
            let edit = Edit {
                range: selection.clone(),
                replacement: String::new(),
                cursor_after: selection.start..selection.start,
            };
            Some(commit(ui, id, text, selection, edit))
        }

        EditorCommand::Paste => {
            let pasted = clipboard.read()?;
            let caret = selection.start + pasted.chars().count();
            let edit = Edit {
                range: selection.clone(),
                replacement: pasted,
                cursor_after: caret..caret,
            };
            Some(commit(ui, id, text, selection, edit))
        }

        EditorCommand::Undo => step_history(ui, id, text, selection, true),
        EditorCommand::Redo => step_history(ui, id, text, selection, false),
        EditorCommand::SelectLine => Some(CommandOutcome {
            selection: editing::select_line(text, selection),
            scroll: false,
            changed: false,
        }),

        EditorCommand::SelectNextOccurrence => {
            occurrence_after(text, selection).map(|selection| CommandOutcome {
                selection,
                scroll: true,
                changed: false,
            })
        }

        EditorCommand::ToggleComment => {
            let edit = editing::toggle_comment(text, selection.clone(), line_comment(language)?)?;
            Some(commit(ui, id, text, selection, edit))
        }

        EditorCommand::DuplicateLine => {
            let edit = editing::duplicate_lines(text, selection.clone());
            Some(commit(ui, id, text, selection, edit))
        }

        EditorCommand::DeleteLine => {
            let edit = editing::delete_lines(text, selection.clone());
            Some(commit(ui, id, text, selection, edit))
        }
    }
}

fn step_history(
    ui: &egui::Ui,
    id: egui::Id,
    text: &mut String,
    selection: Range<usize>,
    undo: bool,
) -> Option<CommandOutcome> {
    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id)?;
    let here = (
        CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end)),
        text.clone(),
    );

    let mut undoer = state.undoer();
    let stepped = if undo {
        undoer.undo(&here).cloned()
    } else {
        undoer.redo(&here).cloned()
    };
    state.set_undoer(undoer);
    state.store(ui.ctx(), id);

    let (range, restored) = stepped?;
    *text = restored;

    let len = text.chars().count();
    let primary = range.primary.index.0.min(len);
    let secondary = range.secondary.index.0.min(len);
    Some(CommandOutcome {
        selection: primary.min(secondary)..primary.max(secondary),
        scroll: false,
        changed: true,
    })
}

fn slice_of(text: &str, range: Range<usize>) -> String {
    text.chars()
        .skip(range.start)
        .take(range.end - range.start)
        .collect()
}

fn commit(
    ui: &egui::Ui,
    id: egui::Id,
    text: &mut String,
    before: Range<usize>,
    edit: Edit,
) -> CommandOutcome {
    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
    push_undo(&mut state, before, text.clone());

    editing::apply(text, &edit);

    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(edit.cursor_after.start),
        CCursor::new(edit.cursor_after.end),
    )));
    state.store(ui.ctx(), id);

    CommandOutcome {
        selection: edit.cursor_after,
        scroll: false,
        changed: true,
    }
}

pub(super) fn push_undo(
    state: &mut egui::text_edit::TextEditState,
    selection: Range<usize>,
    text: String,
) {
    let range = CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end));
    let mut undoer = state.undoer();
    undoer.add_undo(&(range, text));
    state.set_undoer(undoer);
}

pub(super) fn selection_of(ui: &egui::Ui, id: egui::Id, text: &str) -> Option<Range<usize>> {
    let range = egui::text_edit::TextEditState::load(ui.ctx(), id)?
        .cursor
        .char_range()?;
    let len = text.chars().count();
    let primary = range.primary.index.0.min(len);
    let secondary = range.secondary.index.0.min(len);
    Some(primary.min(secondary)..primary.max(secondary))
}

fn occurrence_after(text: &str, selection: Range<usize>) -> Option<Range<usize>> {
    if selection.is_empty() {
        return editing::word_at(text, selection.start);
    }
    let needle: String = text
        .chars()
        .skip(selection.start)
        .take(selection.end - selection.start)
        .collect();
    editing::next_occurrence(text, &needle, selection.end)
}

#[cfg(test)]
mod tests {
    use super::occurrence_after;

    #[test]
    fn cmd_d_takes_the_word_first_then_walks_the_occurrences() {
        let text = "float gain;\nfloat Gain2 = gain;\n";

        let first = occurrence_after(text, 8..8).unwrap();
        assert_eq!(first, 6..10);

        let second = occurrence_after(text, first).unwrap();
        assert_eq!(second, 26..30);

        assert_eq!(occurrence_after(text, second), Some(6..10));
    }

    #[test]
    fn cmd_d_on_whitespace_selects_nothing() {
        assert_eq!(occurrence_after("a  b", 2..2), None);
    }
}
