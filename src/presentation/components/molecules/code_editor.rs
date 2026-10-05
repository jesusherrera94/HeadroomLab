use std::ops::Range;

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};

use crate::application::ports::ClipboardPort;
use crate::domain::editing::{self, Edit, EditorCommand};
use crate::domain::menu;
use crate::domain::text_document::{
    INDENT, Language, auto_indent_for, closing_pair, dedent, line_col_at, line_comment,
};
use crate::presentation::components::atoms::line_numbers::{gutter_width, line_numbers};
use crate::presentation::syntax;

#[derive(Default)]
pub struct CodeEditorOutput {
    pub changed: bool,
    pub cursor: Option<(usize, usize)>,
}

pub struct CodeEditorRequest<'a> {
    pub text: &'a mut String,
    pub language: Language,
    pub highlighted: bool,
    pub id: egui::Id,
    pub select: Option<Range<usize>>,
    pub match_range: Option<Range<usize>>,
    pub clipboard: &'a dyn ClipboardPort,
    pub squiggles: &'a [(Range<usize>, egui::Color32)],
    pub injected: Option<EditorCommand>,
}

pub fn code_editor(ui: &mut egui::Ui, request: CodeEditorRequest<'_>) -> CodeEditorOutput {
    let CodeEditorRequest {
        text,
        language,
        highlighted,
        id,
        select,
        match_range,
        clipboard,
        squiggles,
        injected,
    } = request;

    let pending = rewrite_events(ui, id, text, language, clipboard);

    let gutter = gutter_width(ui, text.lines().count().max(1));
    let mut result = CodeEditorOutput::default();

    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (gutter_rect, _) =
                    ui.allocate_exact_size(egui::vec2(gutter, 0.0), egui::Sense::hover());

                let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _wrap: f32| {
                    syntax::layouter(
                        ui,
                        buffer.as_str(),
                        language,
                        highlighted,
                        match_range.as_ref(),
                    )
                };

                let mut output = egui::TextEdit::multiline(text)
                    .id(id)
                    .font(syntax::code_font())
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter)
                    .show(ui);

                line_numbers(ui, gutter_rect, &output.galley, output.galley_pos.y);

                result.changed = output.response.response.changed();

                let paste_id = id.with("can_paste");
                if output.response.response.secondary_clicked() {
                    let available = clipboard.read().is_some();
                    ui.data_mut(|data| data.insert_temp(paste_id, available));
                }

                let mut chosen = None;
                output.response.response.context_menu(|ui| {
                    let can_paste = ui.data(|data| data.get_temp(paste_id).unwrap_or(false));
                    chosen = editor_menu(ui, text, id, language, can_paste);
                });
                let chosen = chosen.or(injected);

                let mut menu_outcome = None;
                if let Some(command) = chosen {
                    ui.memory_mut(|memory| memory.request_focus(id));
                    menu_outcome = run_command(ui, id, text, language, clipboard, command);
                    result.changed |= menu_outcome.as_ref().is_some_and(|o| o.changed);
                }

                paint_squiggles(ui, &output.galley, output.galley_pos, squiggles);

                if pending.step_back_one
                    && let Some(range) = output.state.cursor.char_range()
                {
                    let back = CCursor::new(range.primary.index.0.saturating_sub(1));
                    output
                        .state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(back)));
                    output.state.clone().store(ui.ctx(), id);
                }

                result.changed |= pending.outcome.as_ref().is_some_and(|o| o.changed);

                let selection = menu_outcome
                    .or(pending.outcome)
                    .map(|outcome| (outcome.selection, outcome.scroll))
                    .or_else(|| select.map(|range| (range, true)));

                if let Some((range, scroll)) = selection {
                    output.state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(range.start),
                        CCursor::new(range.end),
                    )));
                    output.state.clone().store(ui.ctx(), id);
                    if scroll
                        && let Some(row) =
                            output.galley.rows.get(line_col_at(text, range.start).0 - 1)
                    {
                        ui.scroll_to_rect(
                            row.rect().translate(output.galley_pos.to_vec2()),
                            Some(egui::Align::Center),
                        );
                    }
                }

                result.cursor = output
                    .state
                    .cursor
                    .char_range()
                    .map(|range| line_col_at(text, range.primary.index.0));
            });
        });

    result
}

#[derive(Default)]
struct PendingFixups {
    step_back_one: bool,
    outcome: Option<CommandOutcome>,
}

struct CommandOutcome {
    selection: Range<usize>,
    scroll: bool,
    changed: bool,
}

fn rewrite_events(
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

fn paint_squiggles(
    ui: &egui::Ui,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    squiggles: &[(Range<usize>, egui::Color32)],
) {
    const PERIOD: f32 = 4.0;
    const AMPLITUDE: f32 = 1.5;
    const OFFSET: f32 = 1.0;

    let painter = ui.painter();
    for (range, color) in squiggles {
        if range.start >= range.end {
            continue;
        }

        let start = galley.pos_from_cursor(CCursor::new(range.start));
        let end = galley.pos_from_cursor(CCursor::new(range.end));
        let right = if (end.top() - start.top()).abs() < 0.5 {
            end.left()
        } else {
            galley.rect.right()
        };

        let y = galley_pos.y + start.bottom() + OFFSET;
        let left = galley_pos.x + start.left();
        let right = galley_pos.x + right;
        if right <= left {
            continue;
        }

        let mut points = Vec::with_capacity(((right - left) / PERIOD).ceil() as usize + 2);
        let mut x = left;
        let mut up = true;
        while x < right {
            points.push(egui::pos2(x, if up { y } else { y + AMPLITUDE }));
            x += PERIOD / 2.0;
            up = !up;
        }
        points.push(egui::pos2(right, if up { y } else { y + AMPLITUDE }));

        painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, *color)));
    }
}

fn editor_menu(
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

fn run_command(
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

fn push_undo(state: &mut egui::text_edit::TextEditState, selection: Range<usize>, text: String) {
    let range = CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end));
    let mut undoer = state.undoer();
    undoer.add_undo(&(range, text));
    state.set_undoer(undoer);
}

fn selection_of(ui: &egui::Ui, id: egui::Id, text: &str) -> Option<Range<usize>> {
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
    use super::{EditorCommand, apply_dedent, command_for, current_line, occurrence_after};
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
