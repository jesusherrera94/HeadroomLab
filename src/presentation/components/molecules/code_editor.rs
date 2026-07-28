//! The editable code area: line-number gutter + a syntax-highlighted
//! `TextEdit::multiline`, sharing one horizontally-scrolling viewport.
//!
//! egui's own `TextEdit` gets the code-editing keys *almost* right — Tab inserts
//! a literal `\t` and Enter does not indent at all (there is a TODO to that
//! effect in egui itself). Rather than fight the widget after the fact, we
//! rewrite the offending key events in the input queue **before** `TextEdit`
//! sees them, so its cursor handling, selection and undo all stay consistent.

use std::ops::Range;

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};

use crate::domain::text_document::{
    INDENT, Language, auto_indent_for, closing_pair, dedent, line_col_at,
};
use crate::presentation::components::atoms::line_numbers::{gutter_width, line_numbers};
use crate::presentation::syntax;

/// What the user did in the code area this frame.
#[derive(Default)]
pub struct CodeEditorOutput {
    /// The buffer text was modified.
    pub changed: bool,
    /// 1-based cursor position, for the status bar.
    pub cursor: Option<(usize, usize)>,
}

pub struct CodeEditorRequest<'a> {
    pub text: &'a mut String,
    pub language: Language,
    /// False for buffers past the highlight threshold — still editable, just
    /// rendered as plain monospace.
    pub highlighted: bool,
    /// Stable per-file id, so egui keeps cursor, selection, scroll and undo
    /// separately for each open tab.
    pub id: egui::Id,
    /// A character range to select and scroll into view this frame (find hits).
    pub select: Option<Range<usize>>,
}

pub fn code_editor(ui: &mut egui::Ui, request: CodeEditorRequest<'_>) -> CodeEditorOutput {
    let CodeEditorRequest {
        text,
        language,
        highlighted,
        id,
        select,
    } = request;

    // Rewrite Tab / Enter / opening-pair keys before `TextEdit` consumes them.
    let pending = rewrite_events(ui, id, text);

    let gutter = gutter_width(ui, text.lines().count().max(1));
    let mut result = CodeEditorOutput::default();

    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // Reserve the gutter column. Zero height: the numbers are painted
                // afterwards from the galley's own row positions, which is the
                // only way to stay aligned without assuming a row height.
                let (gutter_rect, _) =
                    ui.allocate_exact_size(egui::vec2(gutter, 0.0), egui::Sense::hover());

                let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _wrap: f32| {
                    syntax::layouter(ui, buffer.as_str(), language, highlighted)
                };

                let mut output = egui::TextEdit::multiline(text)
                    .id(id)
                    .font(syntax::code_font())
                    .frame(egui::Frame::NONE)
                    // Tab indents instead of moving focus; we rewrite the event
                    // above so it inserts spaces rather than a tab character.
                    .lock_focus(true)
                    // Clamped to the available width by egui; the galley decides
                    // the real extent, so long lines drive the horizontal scroll.
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter)
                    .show(ui);

                line_numbers(ui, gutter_rect, &output.galley, output.galley_pos.y);

                result.changed = output.response.response.changed();

                // Auto-closed a pair: step back between the two characters.
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

                // A find hit: select it and bring it on screen.
                if let Some(range) = select {
                    output.state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(range.start),
                        CCursor::new(range.end),
                    )));
                    output.state.clone().store(ui.ctx(), id);
                    if let Some(row) = output.galley.rows.get(line_col_at(text, range.start).0 - 1)
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

/// What the event rewrite asked the caller to fix up afterwards.
#[derive(Default)]
struct PendingFixups {
    /// An opening bracket/quote was expanded to a pair, so the cursor must move
    /// back between them once `TextEdit` has inserted the text.
    step_back_one: bool,
}

/// Rewrites this frame's key events so the `TextEdit` behaves like a code
/// editor. Only runs while the editor has focus, so the shortcuts never fire at
/// the explorer or the toolbar.
fn rewrite_events(ui: &egui::Ui, id: egui::Id, text: &mut String) -> PendingFixups {
    let mut pending = PendingFixups::default();
    if !ui.memory(|m| m.has_focus(id)) {
        return pending;
    }

    // The cursor as of the previous frame — enough to know which line Enter is
    // splitting, which is all auto-indent needs.
    let cursor = egui::text_edit::TextEditState::load(ui.ctx(), id)
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0);

    // Shift+Tab is handled here rather than by egui, whose `decrease_indentation`
    // is hardcoded to a 4-space tab stop and so would do nothing to the 2-space
    // indent our Tab inserts. Drop the event so `TextEdit` doesn't see it too.
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
    if dedent_requested
        && let Some(index) = cursor
        && let Some(moved) = apply_dedent(text, index)
    {
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(moved))));
        state.store(ui.ctx(), id);
    }

    ui.input_mut(|input| {
        for event in &mut input.events {
            match event {
                // Tab → our indent unit, rather than egui's literal `\t`.
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.shift && !modifiers.command && !modifiers.alt => {
                    *event = egui::Event::Text(INDENT.to_owned());
                }

                // Enter → newline plus the indentation of the line being left.
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

                // A lone opening bracket or quote → insert the closing one too.
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

/// Removes one indent level from the line containing character offset `cursor`.
/// Returns the cursor's new character offset, or `None` when the line has no
/// leading whitespace to remove.
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

    // Keep the cursor on the same line: never let it slide onto the line above.
    let start_char = text[..start_byte].chars().count();
    Some(cursor.saturating_sub(removed).max(start_char))
}

/// Byte offset of the start of the line containing character offset `index`.
fn line_start_byte(text: &str, index: usize) -> usize {
    let byte = text
        .char_indices()
        .nth(index)
        .map_or(text.len(), |(b, _)| b);
    text[..byte].rfind('\n').map_or(0, |i| i + 1)
}

/// The text of the line containing character offset `index`, up to the cursor —
/// the line Enter is about to split.
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
    use super::{apply_dedent, current_line};

    #[test]
    fn dedent_removes_one_level_and_keeps_the_cursor_on_its_line() {
        // Cursor sits at the end of the indented line (char 15).
        let mut text = String::from("void f() {\n    int a;\n}\n");
        assert_eq!(apply_dedent(&mut text, 21), Some(19));
        assert_eq!(text, "void f() {\n  int a;\n}\n");

        // A second dedent takes it to column 0.
        assert_eq!(apply_dedent(&mut text, 19), Some(17));
        assert_eq!(text, "void f() {\nint a;\n}\n");

        // A third has nothing left to remove.
        assert_eq!(apply_dedent(&mut text, 17), None);
    }

    #[test]
    fn dedent_never_pulls_the_cursor_onto_the_previous_line() {
        // Cursor at the very start of the indented line's content.
        let mut text = String::from("a\n  b\n");
        // Line starts at char 2; removing 2 chars would underflow to char 0.
        assert_eq!(apply_dedent(&mut text, 2), Some(2));
        assert_eq!(text, "a\nb\n");
    }

    #[test]
    fn current_line_returns_text_up_to_the_cursor() {
        let text = "void f() {\n    int a;\n";
        assert_eq!(current_line(text, 10), "void f() {");
        assert_eq!(current_line(text, 21), "    int a;");
        assert_eq!(current_line(text, 0), "");
        // Past the end clamps to the last (empty) line.
        assert_eq!(current_line(text, 999), "");
    }

    #[test]
    fn current_line_counts_characters_not_bytes() {
        let text = "// héllo\nx";
        assert_eq!(current_line(text, 8), "// héllo");
    }
}
