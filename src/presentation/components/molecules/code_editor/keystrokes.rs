//! Rewriting raw key events before the `TextEdit` sees them: editor command
//! chords, Tab/Shift+Tab indentation, auto-indent on Enter and bracket pairing.

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};

use super::commands::{CommandOutcome, push_undo, run_command};
use crate::application::ports::ClipboardPort;
use crate::domain::editing::EditorCommand;
use crate::domain::text_document::{INDENT, Language, auto_indent_for, closing_pair, dedent};

#[derive(Default)]
pub(super) struct PendingFixups {
    pub(super) step_back_one: bool,
    pub(super) outcome: Option<CommandOutcome>,
}

pub(super) fn rewrite_events(
    ui: &egui::Ui,
    id: egui::Id,
    text: &mut String,
    language: Language,
    clipboard: &dyn ClipboardPort,
) -> PendingFixups {
    let mut pending = PendingFixups::default();
    if !ui.memory(|m| m.has_focus(id)) {
        return pending;
    }

    let cursor = egui::text_edit::TextEditState::load(ui.ctx(), id)
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0);

    for command in take_commands(ui) {
        if let Some(outcome) = run_command(ui, id, text, language, clipboard, command) {
            pending.outcome = Some(outcome);
        }
    }

    let mut dedent_requested = false;
    ui.input_mut(|input| {
        input.events.retain(|event| {
            let is_shift_tab = matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.shift
            );
            dedent_requested |= is_shift_tab;
            !is_shift_tab
        });
    });
    if dedent_requested && let Some(index) = cursor {
        let before = text.clone();
        if let Some(moved) = apply_dedent(text, index) {
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            push_undo(&mut state, index..index, before);
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(moved))));
            state.store(ui.ctx(), id);
        }
    }

    ui.input_mut(|input| {
        for event in &mut input.events {
            match event {
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.shift && !modifiers.command && !modifiers.alt => {
                    *event = egui::Event::Text(INDENT.to_owned());
                }

                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.is_none() => {
                    if let Some(index) = cursor {
                        let indent = auto_indent_for(current_line(text, index));
                        if !indent.is_empty() {
                            *event = egui::Event::Text(format!("\n{indent}"));
                        }
                    }
                }

                egui::Event::Text(typed) => {
                    let mut chars = typed.chars();
                    if let (Some(open), None) = (chars.next(), chars.next())
                        && let Some(close) = closing_pair(open)
                    {
                        *typed = format!("{open}{close}");
                        pending.step_back_one = true;
                    }
                }

                _ => {}
            }
        }
    });

    pending
}

fn take_commands(ui: &egui::Ui) -> Vec<EditorCommand> {
    let mut commands = Vec::new();
    ui.input_mut(|input| {
        input.events.retain(|event| match command_for(event) {
            Some(command) => {
                commands.push(command);
                false
            }
            None => true,
        });
    });
    commands
}

fn command_for(event: &egui::Event) -> Option<EditorCommand> {
    let egui::Event::Key {
        key,
        pressed: true,
        modifiers,
        ..
    } = event
    else {
        return None;
    };

    if modifiers.shift && modifiers.alt && !modifiers.command && *key == egui::Key::ArrowDown {
        return Some(EditorCommand::DuplicateLine);
    }
    if !modifiers.command || modifiers.alt {
        return None;
    }

    match key {
        egui::Key::L if !modifiers.shift => Some(EditorCommand::SelectLine),
        egui::Key::D if !modifiers.shift => Some(EditorCommand::SelectNextOccurrence),
        egui::Key::K if modifiers.shift => Some(EditorCommand::DeleteLine),
        egui::Key::Slash => Some(EditorCommand::ToggleComment),
        _ => None,
    }
}

fn apply_dedent(text: &mut String, cursor: usize) -> Option<usize> {
    let start_byte = line_start_byte(text, cursor);
    let end_byte = text[start_byte..]
        .find('\n')
        .map_or(text.len(), |i| start_byte + i);

    let (line, removed) = dedent(&text[start_byte..end_byte]);
    if removed == 0 {
        return None;
    }
    text.replace_range(start_byte..end_byte, &line);

    let start_char = text[..start_byte].chars().count();
    Some(cursor.saturating_sub(removed).max(start_char))
}

fn line_start_byte(text: &str, index: usize) -> usize {
    let byte = text
        .char_indices()
        .nth(index)
        .map_or(text.len(), |(b, _)| b);
    text[..byte].rfind('\n').map_or(0, |i| i + 1)
}

fn current_line(text: &str, index: usize) -> &str {
    let byte = text
        .char_indices()
        .nth(index)
        .map(|(b, _)| b)
        .unwrap_or(text.len());
    let start = text[..byte].rfind('\n').map_or(0, |i| i + 1);
    &text[start..byte]
}

#[cfg(test)]
mod tests {
    use super::{EditorCommand, apply_dedent, command_for, current_line};
    use eframe::egui::{self, Modifiers};

    fn press(key: egui::Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn the_command_chords_map_to_their_commands() {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND.plus(Modifiers::SHIFT);
        let shift_alt = Modifiers::SHIFT.plus(Modifiers::ALT);

        assert_eq!(
            command_for(&press(egui::Key::L, cmd)),
            Some(EditorCommand::SelectLine)
        );
        assert_eq!(
            command_for(&press(egui::Key::D, cmd)),
            Some(EditorCommand::SelectNextOccurrence)
        );
        assert_eq!(
            command_for(&press(egui::Key::Slash, cmd)),
            Some(EditorCommand::ToggleComment)
        );
        assert_eq!(
            command_for(&press(egui::Key::K, cmd_shift)),
            Some(EditorCommand::DeleteLine)
        );
        assert_eq!(
            command_for(&press(egui::Key::ArrowDown, shift_alt)),
            Some(EditorCommand::DuplicateLine)
        );
    }

    #[test]
    fn toggle_comment_ignores_shift_because_slash_is_a_shifted_key_on_many_layouts() {
        assert_eq!(
            command_for(&press(
                egui::Key::Slash,
                Modifiers::COMMAND.plus(Modifiers::SHIFT)
            )),
            Some(EditorCommand::ToggleComment)
        );
    }

    #[test]
    fn alt_gr_combinations_never_trip_a_command() {
        let alt_gr = Modifiers::COMMAND.plus(Modifiers::ALT);
        for key in [egui::Key::L, egui::Key::D, egui::Key::Slash, egui::Key::K] {
            assert_eq!(
                command_for(&press(key, alt_gr)),
                None,
                "{key:?} fired on AltGr"
            );
        }
    }

    #[test]
    fn unmodified_and_released_keys_are_left_alone() {
        assert_eq!(command_for(&press(egui::Key::L, Modifiers::NONE)), None);
        assert_eq!(command_for(&press(egui::Key::D, Modifiers::SHIFT)), None);
        assert_eq!(
            command_for(&press(egui::Key::ArrowDown, Modifiers::SHIFT)),
            None
        );
        assert_eq!(
            command_for(&egui::Event::Key {
                key: egui::Key::L,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }),
            None
        );
        assert_eq!(command_for(&egui::Event::Text("l".into())), None);
    }

    #[test]
    fn dedent_removes_one_level_and_keeps_the_cursor_on_its_line() {
        let mut text = String::from("void f() {\n    int a;\n}\n");
        assert_eq!(apply_dedent(&mut text, 21), Some(19));
        assert_eq!(text, "void f() {\n  int a;\n}\n");

        assert_eq!(apply_dedent(&mut text, 19), Some(17));
        assert_eq!(text, "void f() {\nint a;\n}\n");

        assert_eq!(apply_dedent(&mut text, 17), None);
    }

    #[test]
    fn dedent_never_pulls_the_cursor_onto_the_previous_line() {
        let mut text = String::from("a\n  b\n");
        assert_eq!(apply_dedent(&mut text, 2), Some(2));
        assert_eq!(text, "a\nb\n");
    }

    #[test]
    fn current_line_returns_text_up_to_the_cursor() {
        let text = "void f() {\n    int a;\n";
        assert_eq!(current_line(text, 10), "void f() {");
        assert_eq!(current_line(text, 21), "    int a;");
        assert_eq!(current_line(text, 0), "");
        assert_eq!(current_line(text, 999), "");
    }

    #[test]
    fn current_line_counts_characters_not_bytes() {
        let text = "// héllo\nx";
        assert_eq!(current_line(text, 8), "// héllo");
    }
}
