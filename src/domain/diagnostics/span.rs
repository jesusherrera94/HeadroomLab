//! Mapping a compiler's line/byte-column position onto a character range in
//! the editor buffer, for squiggles and jump-to.

use std::ops::Range;

use crate::domain::editing;

pub fn byte_column_to_char(line: &str, column: u32) -> usize {
    let byte = (column.max(1) - 1) as usize;
    if byte >= line.len() {
        return line.chars().count();
    }
    let mut boundary = byte;
    while boundary > 0 && !line.is_char_boundary(boundary) {
        boundary -= 1;
    }
    line[..boundary].chars().count()
}

const FALLBACK_SPAN: usize = 3;

pub fn span_in(text: &str, line: u32, column: Option<u32>) -> Option<Range<usize>> {
    let line_index = (line.max(1) - 1) as usize;

    let mut line_start = 0usize;
    let mut current = 0usize;
    for (index, line_text) in text.split('\n').enumerate() {
        if index == line_index {
            line_start = current;
            let column_chars = column.map_or(0, |c| byte_column_to_char(line_text, c));
            return Some(span_within_line(
                line_text,
                line_start,
                column_chars.min(line_text.chars().count()),
            ));
        }
        current += line_text.chars().count() + 1; // +1 for the newline
    }

    let _ = line_start;
    None
}

fn span_within_line(line_text: &str, line_start: usize, column: usize) -> Range<usize> {
    let length = line_text.chars().count();

    if let Some(word) = editing::word_at(line_text, column) {
        return (line_start + word.start)..(line_start + word.end);
    }

    let start = column.min(length);
    let end = (start + FALLBACK_SPAN).min(length).max(start);
    (line_start + start)..(line_start + end)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- columns ------------------------------------------------------------

    #[test]
    fn a_byte_column_becomes_a_character_offset() {
        // "  float cutof" — column 9 (1-based bytes) is the 'c' of cutof.
        let line = "  float cutof = 0.5f;";
        assert_eq!(byte_column_to_char(line, 9), 8);
        assert_eq!(&line[8..13], "cutof");
    }

    #[test]
    fn multi_byte_characters_shift_the_column() {
        // "// é" is 5 bytes but 4 characters; a byte column past the é must not
        // be used as a character offset directly.
        let line = "// é x";
        // Byte 6 (1-based) is the 'x'; as characters that is offset 5.
        assert_eq!(byte_column_to_char(line, 7), 5);
        assert_eq!(line.chars().nth(5), Some('x'));
    }

    // -- spans --------------------------------------------------------------

    const SOURCE: &str = "void f() {\n  float cutof = 0.5f;\n  return;\n}\n";

    #[test]
    fn a_span_covers_the_identifier_at_the_column() {
        // Line 2, byte column 9 is the 'c' of `cutof`.
        let span = span_in(SOURCE, 2, Some(9)).unwrap();
        let text: String = SOURCE.chars().take(span.end).skip(span.start).collect();
        assert_eq!(text, "cutof", "the squiggle must cover the whole token");
    }

    #[test]
    fn a_span_on_punctuation_falls_back_to_a_short_run() {
        // Line 2 column 15 is the '=' — not an identifier character.
        let span = span_in(SOURCE, 2, Some(15)).unwrap();
        assert_eq!(span.len(), FALLBACK_SPAN);
        let text: String = SOURCE.chars().take(span.end).skip(span.start).collect();
        assert_eq!(text, "= 0");
    }

    #[test]
    fn a_span_without_a_column_starts_at_the_line() {
        let span = span_in(SOURCE, 3, None).unwrap();
        // Column 0 of "  return;" is a space, so the fallback run applies.
        let text: String = SOURCE.chars().take(span.end).skip(span.start).collect();
        assert_eq!(text, "  r");
    }

    #[test]
    fn a_span_past_the_end_of_the_line_clamps_rather_than_panicking() {
        let span = span_in(SOURCE, 3, Some(500)).unwrap();
        assert!(span.start <= SOURCE.chars().count());
        assert!(span.end <= SOURCE.chars().count());
        assert!(span.start <= span.end);
    }

    #[test]
    fn a_line_the_buffer_does_not_have_yields_nothing() {
        // The file may have been edited, or shrunk, since the build.
        assert_eq!(span_in(SOURCE, 999, Some(1)), None);
        assert_eq!(span_in("", 5, Some(1)), None);
    }

    #[test]
    fn spans_are_character_offsets_even_with_multi_byte_text() {
        let source = "// héllo\n  float cutof = 0;\n";
        let span = span_in(source, 2, Some(9)).unwrap();
        let text: String = source.chars().take(span.end).skip(span.start).collect();
        assert_eq!(
            text, "cutof",
            "a multi-byte first line must not shift the second line's span"
        );
    }

    #[test]
    fn out_of_range_columns_clamp_instead_of_panicking() {
        let line = "short";
        assert_eq!(byte_column_to_char(line, 999), 5);
        // Compilers are 1-based; 0 would underflow.
        assert_eq!(byte_column_to_char(line, 0), 0);
        assert_eq!(byte_column_to_char("", 1), 0);
    }
}
