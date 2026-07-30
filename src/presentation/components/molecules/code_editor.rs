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

use crate::application::ports::ClipboardPort;
use crate::domain::editing::{self, Edit, EditorCommand};
use crate::domain::text_document::{
    INDENT, Language, auto_indent_for, closing_pair, dedent, line_col_at, line_comment,
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
    /// The find bar's current match, tinted in the layout for as long as the bar
    /// is open. Separate from `select` because egui only paints a selection
    /// while the widget has focus, and during a find the focus is in the query
    /// field.
    pub match_range: Option<Range<usize>>,
    /// Read side of the system clipboard, for the context menu's Paste. The
    /// keyboard `⌘V` never comes through here — egui gets that as an OS event.
    pub clipboard: &'a dyn ClipboardPort,
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
    } = request;

    // Rewrite Tab / Enter / opening-pair keys, and run the editor commands,
    // before `TextEdit` consumes them.
    let pending = rewrite_events(ui, id, text, language, clipboard);

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

                // The clipboard is read once, as the menu opens, rather than on
                // every frame it stays open: on X11 each read is a round-trip to
                // the owning process.
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

                // Run it here, where the `TextEdit`'s borrow of `text` is over.
                // Menu items raise the same `EditorCommand`s the key chords do,
                // so the two paths cannot drift apart in behaviour.
                let mut menu_outcome = None;
                if let Some(command) = chosen {
                    // Opening the menu took focus off the editor, and egui paints
                    // a `TextEdit`'s selection *only while it is focused*
                    // (`text_edit/builder.rs:833`). Without handing focus back, a
                    // command like Select Line would set a selection that is
                    // never drawn — it looks like the item did nothing.
                    ui.memory_mut(|memory| memory.request_focus(id));
                    menu_outcome = run_command(ui, id, text, language, clipboard, command);
                    result.changed |= menu_outcome.is_some();
                }

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

                // A selection to apply now the widget has run: a command's
                // result, or a find hit. A command wins — the two cannot both
                // occur in one frame, since a find selection only ever arises
                // from interacting with the find bar.
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

/// What the event rewrite asked the caller to fix up afterwards.
#[derive(Default)]
struct PendingFixups {
    /// An opening bracket/quote was expanded to a pair, so the cursor must move
    /// back between them once `TextEdit` has inserted the text.
    step_back_one: bool,
    /// A command ran and wants the selection put somewhere specific.
    outcome: Option<CommandOutcome>,
}

/// Where a command left the selection, and whether it needs bringing on screen.
/// Only `⌘D` scrolls: the rest act at the cursor, which is already in view, and
/// re-centring the pane on every comment toggle would be its own annoyance.
struct CommandOutcome {
    selection: Range<usize>,
    scroll: bool,
}

/// Rewrites this frame's key events so the `TextEdit` behaves like a code
/// editor. Only runs while the editor has focus, so the shortcuts never fire at
/// the explorer or the toolbar.
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

    // The cursor as of the previous frame — enough to know which line Enter is
    // splitting, which is all auto-indent needs.
    let cursor = egui::text_edit::TextEditState::load(ui.ctx(), id)
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0);

    // Editor commands first: they are matched and dropped from the queue before
    // anything else looks at it. `⇧⌥↓` in particular *must* be removed — egui's
    // `move_single_cursor` ignores alt on the vertical arrows, so leaving it in
    // would shift-extend the selection downward as well as duplicating.
    for command in take_commands(ui) {
        if let Some(outcome) = run_command(ui, id, text, language, clipboard, command) {
            pending.outcome = Some(outcome);
        }
    }

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
    if dedent_requested && let Some(index) = cursor {
        // Captured before the mutation so the dedent becomes its own undo step.
        // `apply_dedent` edits `text` directly, outside `TextEdit`'s event flow,
        // so without this the undoer's timed coalescing could fold it into an
        // unrelated typing step.
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

/// Right-click menu for the code area. Returns the command the user picked.
///
/// Every item is disabled when it cannot act, so the menu never offers something
/// that would silently do nothing — the same rule the tab strip's menu follows.
/// Toggle Comment is here as well as on `⌘/` because layouts that need AltGr for
/// `/` cannot produce that chord at all, and this is their only way to reach it.
///
/// The menu deliberately does not move the caret first: egui's `TextEdit` ignores
/// secondary clicks for cursor placement, so these act on whatever was already
/// selected.
fn editor_menu(
    ui: &mut egui::Ui,
    text: &str,
    id: egui::Id,
    language: Language,
    can_paste: bool,
) -> Option<EditorCommand> {
    ui.set_min_width(220.0);

    let keys = Shortcuts::for_platform();
    let selection = selection_of(ui, id, text).unwrap_or(0..0);
    let has_selection = !selection.is_empty();

    // `has_undo`/`has_redo` are asked against the live buffer, so the items grey
    // out the moment there is nothing left to step through.
    let state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
    let here = (
        CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end)),
        text.to_owned(),
    );
    let undoer = state.undoer();

    let mut chosen = None;
    let mut item = |ui: &mut egui::Ui, enabled, label, shortcut, command| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(label)
                    .shortcut_text(shortcut)
                    .frame(false),
            )
            .clicked()
        {
            chosen = Some(command);
            ui.close();
        }
    };

    item(ui, has_selection, "Cut", keys.cut, EditorCommand::Cut);
    item(ui, has_selection, "Copy", keys.copy, EditorCommand::Copy);
    item(ui, can_paste, "Paste", keys.paste, EditorCommand::Paste);
    ui.separator();
    item(
        ui,
        undoer.has_undo(&here),
        "Undo",
        keys.undo,
        EditorCommand::Undo,
    );
    item(
        ui,
        undoer.has_redo(&here),
        "Redo",
        keys.redo,
        EditorCommand::Redo,
    );
    ui.separator();
    item(
        ui,
        true,
        "Select Line",
        keys.select_line,
        EditorCommand::SelectLine,
    );
    item(
        ui,
        line_comment(language).is_some(),
        "Toggle Comment",
        keys.comment,
        EditorCommand::ToggleComment,
    );
    item(
        ui,
        true,
        "Select All",
        keys.select_all,
        EditorCommand::SelectAll,
    );

    chosen
}

/// How the editor's chords are written for this platform. macOS uses the glyphs;
/// Windows and Linux share the spelled-out form.
struct Shortcuts {
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    undo: &'static str,
    redo: &'static str,
    select_line: &'static str,
    comment: &'static str,
    select_all: &'static str,
}

impl Shortcuts {
    fn for_platform() -> Self {
        if cfg!(target_os = "macos") {
            Self {
                cut: "⌘X",
                copy: "⌘C",
                paste: "⌘V",
                undo: "⌘Z",
                redo: "⇧⌘Z",
                select_line: "⌘L",
                comment: "⌘/",
                select_all: "⌘A",
            }
        } else {
            Self {
                cut: "Ctrl+X",
                copy: "Ctrl+C",
                paste: "Ctrl+V",
                undo: "Ctrl+Z",
                redo: "Ctrl+Shift+Z",
                select_line: "Ctrl+L",
                comment: "Ctrl+/",
                select_all: "Ctrl+A",
            }
        }
    }
}

/// Matches this frame's key events against the editor's command chords, removing
/// every one that matches from the queue so `TextEdit` never sees it.
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

/// The command a key event stands for, if any.
///
/// `Modifiers::COMMAND` is Cmd on macOS and Ctrl elsewhere, so one arm covers
/// all three platforms. Every command arm requires `!alt`, because on Windows
/// and Linux `command == ctrl` and AltGr arrives as ctrl+alt — without the
/// guard, AltGr combos on European layouts would trip these shortcuts.
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

    // ⇧⌥↓ — the one chord built on alt rather than guarded against it.
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
        // Shift is deliberately ignored: `/` is a shifted key on the German,
        // Spanish and French layouts, among others.
        egui::Key::Slash => Some(EditorCommand::ToggleComment),
        _ => None,
    }
}

/// Runs one command against the buffer. Text-changing commands record an undo
/// point first, so each is undone by a single `⌘Z` (AC 9).
///
/// Undo, redo, cut, copy, paste and select-all reach this function only from the
/// context menu: `take_commands` never matches their chords, because egui's own
/// `TextEdit` already binds them correctly and there is nothing to improve on.
fn run_command(
    ui: &egui::Ui,
    id: egui::Id,
    text: &mut String,
    language: Language,
    clipboard: &dyn ClipboardPort,
    command: EditorCommand,
) -> Option<CommandOutcome> {
    // A buffer that has never been clicked into has no stored cursor at all.
    // Treat that as a caret at the top rather than dropping the command: the
    // menu can be opened without ever having focused the editor, and silently
    // doing nothing is exactly the bug that reads as "the menu is broken".
    let selection = selection_of(ui, id, text).unwrap_or(0..0);

    match command {
        EditorCommand::SelectAll => Some(CommandOutcome {
            selection: 0..text.chars().count(),
            scroll: false,
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
        }),

        EditorCommand::SelectNextOccurrence => {
            occurrence_after(text, selection).map(|selection| CommandOutcome {
                selection,
                scroll: true,
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

/// Menu-driven undo/redo. Drives the same `TextEditUndoer` the `⌘Z` key does, so
/// the two share one history rather than keeping rival stacks.
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
    })
}

/// The text of a character range — the selection, for cut and copy.
fn slice_of(text: &str, range: Range<usize>) -> String {
    text.chars()
        .skip(range.start)
        .take(range.end - range.start)
        .collect()
}

/// Applies `edit`, recording the pre-edit buffer as an undo point first.
///
/// The new cursor is stored here as well as being returned. On the keyboard path
/// this runs *before* the `TextEdit` does, so leaving the old cursor in place
/// would hand the widget a position derived from text that no longer exists —
/// past the end of the buffer, after a delete-line on the last line.
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
    }
}

/// Pushes `(selection, text)` onto the undo stack as a discrete step.
///
/// `add_undo` also clears the undoer's in-flight "flux", so a burst of typing
/// that had not yet been committed becomes its own undo point rather than being
/// merged with the command that follows it.
fn push_undo(state: &mut egui::text_edit::TextEditState, selection: Range<usize>, text: String) {
    let range = CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end));
    let mut undoer = state.undoer();
    undoer.add_undo(&(range, text));
    state.set_undoer(undoer);
}

/// The current selection as a sorted character range. egui's primary cursor can
/// sit either side of its secondary, depending on which way the user dragged.
fn selection_of(ui: &egui::Ui, id: egui::Id, text: &str) -> Option<Range<usize>> {
    let range = egui::text_edit::TextEditState::load(ui.ctx(), id)?
        .cursor
        .char_range()?;
    let len = text.chars().count();
    let primary = range.primary.index.0.min(len);
    let secondary = range.secondary.index.0.min(len);
    Some(primary.min(secondary)..primary.max(secondary))
}

/// `⌘D`: the word under the cursor on the first press, then each following
/// occurrence of whatever is selected.
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
    use super::{EditorCommand, apply_dedent, command_for, current_line, occurrence_after};
    use eframe::egui::{self, Modifiers};

    /// A key-press event with the given modifiers, as the input queue delivers
    /// it. `Modifiers::COMMAND` is Cmd on macOS and Ctrl elsewhere, so these
    /// tests assert the same behaviour the running platform will see.
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
        // On Windows and Linux `command == ctrl`, and AltGr arrives as ctrl+alt.
        // Without the `!alt` guard these would fire while typing on European
        // layouts.
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
        // A plain ⇧↓ is egui's own extend-selection-down, not a duplicate.
        assert_eq!(
            command_for(&press(egui::Key::ArrowDown, Modifiers::SHIFT)),
            None
        );
        // Releases are not commands.
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
        // Nor is anything that isn't a key at all.
        assert_eq!(command_for(&egui::Event::Text("l".into())), None);
    }

    #[test]
    fn cmd_d_takes_the_word_first_then_walks_the_occurrences() {
        let text = "float gain;\nfloat Gain2 = gain;\n";

        // Caret inside the first `gain` → select that word.
        let first = occurrence_after(text, 8..8).unwrap();
        assert_eq!(first, 6..10);

        // Again → the next exact, whole-word hit, skipping `Gain2`.
        let second = occurrence_after(text, first).unwrap();
        assert_eq!(second, 26..30);

        // Again → wraps back to the top.
        assert_eq!(occurrence_after(text, second), Some(6..10));
    }

    #[test]
    fn cmd_d_on_whitespace_selects_nothing() {
        assert_eq!(occurrence_after("a  b", 2..2), None);
    }

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
