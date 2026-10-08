//! Whole-line operations: the span a selection touches, Select Line,
//! Duplicate Line and Delete Line.

use std::ops::Range;

use super::{Edit, line_end, line_start, slice, sorted};

pub fn line_span(text: &str, selection: Range<usize>) -> Range<usize> {
    let len = text.chars().count();
    let (lo, hi) = sorted(selection, len);

    let start = line_start(text, lo);
    let hi = if hi > lo && hi == line_start(text, hi) {
        hi - 1
    } else {
        hi
    };
    start..line_end(text, hi)
}

pub fn select_line(text: &str, selection: Range<usize>) -> Range<usize> {
    let len = text.chars().count();
    let (lo, hi) = sorted(selection, len);
    let span = line_span(text, lo..hi);

    if lo == span.start && hi == span.end && span.end < len {
        return span.start..line_end(text, span.end + 1);
    }
    span
}

pub fn duplicate_lines(text: &str, selection: Range<usize>) -> Edit {
    let span = line_span(text, selection);
    let block = slice(text, span.clone());
    let block_len = block.chars().count();

    Edit {
        range: span.end..span.end,
        replacement: format!("\n{block}"),
        cursor_after: (span.end + 1)..(span.end + 1 + block_len),
    }
}

pub fn delete_lines(text: &str, selection: Range<usize>) -> Edit {
    let len = text.chars().count();
    let span = line_span(text, selection);

    let range = if span.end < len {
        span.start..span.end + 1
    } else if span.start > 0 {
        span.start - 1..span.end
    } else {
        span.start..span.end
    };
    let caret = range.start;

    Edit {
        range,
        replacement: String::new(),
        cursor_after: caret..caret,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::editing::applied;

    #[test]
    fn line_span_covers_the_whole_lines_a_selection_touches() {
        let text = "one\ntwo\nthree\n";
        assert_eq!(line_span(text, 0..0), 0..3);
        assert_eq!(line_span(text, 5..6), 4..7);
        assert_eq!(line_span(text, 1..5), 0..7);
        assert_eq!(line_span(text, 1..9), 0..13);
        assert_eq!(line_span(text, 14..14), 14..14);
    }

    #[test]
    fn line_span_ignores_a_selection_that_only_reaches_the_next_line_start() {
        let text = "one\ntwo\nthree\n";
        assert_eq!(line_span(text, 0..4), 0..3);
        assert_eq!(line_span(text, 4..4), 4..7);
    }

    #[test]
    fn line_span_accepts_a_backwards_selection() {
        let backwards = |start, end| Range { start, end };
        let text = "one\ntwo\nthree\n";
        assert_eq!(line_span(text, backwards(5, 1)), 0..7);
        assert_eq!(line_span(text, backwards(9, 1)), 0..13);
    }

    #[test]
    fn select_line_takes_the_line_then_extends_downward() {
        let text = "one\ntwo\nthree";
        let first = select_line(text, 1..1);
        assert_eq!(first, 0..3);
        let second = select_line(text, first);
        assert_eq!(second, 0..7);
        let third = select_line(text, second);
        assert_eq!(third, 0..13);
        assert_eq!(select_line(text, third.clone()), third);
    }

    #[test]
    fn select_line_from_either_end_of_the_line_selects_it_first() {
        let text = "one\ntwo\n";
        assert_eq!(select_line(text, 4..4), 4..7);
        assert_eq!(select_line(text, 7..7), 4..7);
    }

    #[test]
    fn select_line_on_an_empty_line_extends_to_the_next_one() {
        let text = "a\n\nb\n";
        assert_eq!(select_line(text, 2..2), 2..4);
    }

    #[test]
    fn duplicate_copies_the_line_below_and_selects_the_copy() {
        let text = "one\ntwo\n";
        let (out, cursor) = applied(text, &duplicate_lines(text, 1..1));
        assert_eq!(out, "one\none\ntwo\n");
        assert_eq!(cursor, 4..7);
        assert_eq!(&out[4..7], "one");
    }

    #[test]
    fn duplicate_covers_every_line_the_selection_touches() {
        let text = "a\nb\nc\n";
        let (out, cursor) = applied(text, &duplicate_lines(text, 0..3));
        assert_eq!(out, "a\nb\na\nb\nc\n");
        assert_eq!(cursor, 4..7);
    }

    #[test]
    fn duplicate_works_on_a_last_line_with_no_trailing_newline() {
        let text = "a\nb";
        let (out, cursor) = applied(text, &duplicate_lines(text, 3..3));
        assert_eq!(out, "a\nb\nb");
        assert_eq!(cursor, 4..5);
    }

    #[test]
    fn duplicating_a_duplicate_is_idempotent_in_shape() {
        let text = "x\n";
        let first = duplicate_lines(text, 0..0);
        let (out, cursor) = applied(text, &first);
        assert_eq!(out, "x\nx\n");
        let (out, _) = applied(&out, &duplicate_lines(&out, cursor));
        assert_eq!(out, "x\nx\nx\n");
    }

    #[test]
    fn delete_takes_the_following_newline_so_no_blank_line_is_left() {
        let text = "one\ntwo\nthree\n";
        let (out, cursor) = applied(text, &delete_lines(text, 5..5));
        assert_eq!(out, "one\nthree\n");
        assert_eq!(cursor, 4..4);
    }

    #[test]
    fn deleting_the_last_line_takes_the_preceding_newline() {
        let text = "one\ntwo";
        let (out, cursor) = applied(text, &delete_lines(text, 6..6));
        assert_eq!(out, "one");
        assert_eq!(cursor, 3..3);
    }

    #[test]
    fn deleting_the_only_line_empties_the_buffer() {
        let text = "only";
        let (out, cursor) = applied(text, &delete_lines(text, 2..2));
        assert_eq!(out, "");
        assert_eq!(cursor, 0..0);
    }

    #[test]
    fn delete_removes_every_line_the_selection_touches() {
        let text = "a\nb\nc\nd\n";
        let (out, _) = applied(text, &delete_lines(text, 2..5));
        assert_eq!(out, "a\nd\n");
    }
}
