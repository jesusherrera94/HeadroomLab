mod comment;
mod lines;
mod search;

use std::ops::Range;

pub use comment::toggle_comment;
pub use lines::{delete_lines, duplicate_lines, line_span, select_line};
pub use search::{find_matches, next_occurrence, word_at};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    SelectLine,
    SelectNextOccurrence,
    ToggleComment,
    DuplicateLine,
    DeleteLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
    pub cursor_after: Range<usize>,
}

pub fn apply(text: &mut String, edit: &Edit) {
    let start = byte_of_char(text, edit.range.start);
    let end = byte_of_char(text, edit.range.end);
    text.replace_range(start..end, &edit.replacement);
}

fn sorted(selection: Range<usize>, len: usize) -> (usize, usize) {
    let lo = selection.start.min(selection.end).min(len);
    let hi = selection.start.max(selection.end).min(len);
    (lo, hi)
}

fn byte_of_char(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn char_of_byte(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn slice(text: &str, range: Range<usize>) -> &str {
    &text[byte_of_char(text, range.start)..byte_of_char(text, range.end)]
}

fn line_start(text: &str, index: usize) -> usize {
    let byte = byte_of_char(text, index);
    text[..byte]
        .rfind('\n')
        .map_or(0, |i| char_of_byte(text, i + 1))
}

fn line_end(text: &str, index: usize) -> usize {
    let byte = byte_of_char(text, index);
    text[byte..]
        .find('\n')
        .map_or_else(|| text.chars().count(), |i| char_of_byte(text, byte + i))
}

/// Applies `edit` to a copy of `text`, returning the result and where the
/// cursor lands — the shape every edit test asserts on.
#[cfg(test)]
fn applied(text: &str, edit: &Edit) -> (String, Range<usize>) {
    let mut out = text.to_owned();
    apply(&mut out, edit);
    (out, edit.cursor_after.clone())
}
