use std::ops::Range;

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

pub fn word_at(text: &str, index: usize) -> Option<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let index = index.min(chars.len());

    let anchor = if index < chars.len() && is_word(chars[index]) {
        index
    } else if index > 0 && is_word(chars[index - 1]) {
        index - 1
    } else {
        return None;
    };

    let mut start = anchor;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = anchor + 1;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    Some(start..end)
}

pub fn next_occurrence(text: &str, needle: &str, from: usize) -> Option<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return None;
    }

    let last = chars.len() - needle.len();
    let begin = if from > last { 0 } else { from };
    let positions = last + 1;

    (0..positions)
        .map(|step| (begin + step) % positions)
        .find(|&at| chars[at..at + needle.len()] == needle[..] && bounded(&chars, at, &needle))
        .map(|at| at..at + needle.len())
}

pub fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let haystack = text.to_lowercase();
    let needle = query.to_lowercase();

    let mut char_of_byte = vec![0usize; haystack.len() + 1];
    for (chars, (byte, _)) in haystack.char_indices().enumerate() {
        char_of_byte[byte] = chars;
    }
    char_of_byte[haystack.len()] = haystack.chars().count();

    let query_chars = needle.chars().count();
    let mut matches = Vec::new();
    let mut from = 0;
    while let Some(found) = haystack[from..].find(&needle) {
        let byte = from + found;
        let start = char_of_byte[byte];
        matches.push(start..start + query_chars);
        from = byte + needle.len().max(1);
    }
    matches
}

pub fn toggle_comment(text: &str, selection: Range<usize>, token: &str) -> Option<Edit> {
    let len = text.chars().count();
    let (lo, hi) = sorted(selection, len);
    let span = line_span(text, lo..hi);
    let block = slice(text, span.clone());

    let lines: Vec<&str> = block.split('\n').collect();
    if lines.iter().all(|line| line.trim().is_empty()) {
        return None;
    }

    let commented = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with(token));

    let column = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indent_chars(line))
        .min()
        .unwrap_or(0);

    let mut out = String::with_capacity(block.len() + lines.len() * (token.len() + 1));
    let mut changes: Vec<(isize, usize)> = Vec::with_capacity(lines.len());

    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            out.push_str(line);
            changes.push((0, usize::MAX));
            continue;
        }

        if commented {
            let at = indent_chars(line);
            let rest = &line[byte_of_char(line, at)..];
            let stripped = rest
                .strip_prefix(token)
                .map(|r| r.strip_prefix(' ').unwrap_or(r))
                .unwrap_or(rest);
            let removed = rest.chars().count() - stripped.chars().count();
            out.push_str(&line[..byte_of_char(line, at)]);
            out.push_str(stripped);
            changes.push((-(removed as isize), at));
        } else {
            let at = byte_of_char(line, column);
            out.push_str(&line[..at]);
            out.push_str(token);
            out.push(' ');
            out.push_str(&line[at..]);
            changes.push(((token.chars().count() + 1) as isize, column));
        }
    }

    let cursor_after = if lo == hi {
        let caret = carry_caret(block, span.start, lo, &changes);
        caret..caret
    } else {
        span.start..span.start + out.chars().count()
    };

    Some(Edit {
        range: span,
        replacement: out,
        cursor_after,
    })
}

/// Where a caret at `caret` lands after a per-line edit of `block`: shifted by
/// every earlier line's change, plus its own line's when the caret sits at or
/// after the column that changed.
fn carry_caret(block: &str, block_start: usize, caret: usize, changes: &[(isize, usize)]) -> usize {
    let offset = caret - block_start;
    let before: String = block.chars().take(offset).collect();
    let line = before.matches('\n').count();
    let column = before.chars().count()
        - before
            .rfind('\n')
            .map_or(0, |i| before[..=i].chars().count());

    let mut shift: isize = changes[..line].iter().map(|(delta, _)| delta).sum();
    if let Some(&(delta, at)) = changes.get(line)
        && column >= at
    {
        shift += delta;
    }

    caret.saturating_add_signed(shift).max(caret - column)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn bounded(chars: &[char], at: usize, needle: &[char]) -> bool {
    let start_ok = !is_word(needle[0]) || at == 0 || !is_word(chars[at - 1]);
    let end = at + needle.len();
    let end_ok = !is_word(needle[needle.len() - 1]) || end >= chars.len() || !is_word(chars[end]);
    start_ok && end_ok
}

fn sorted(selection: Range<usize>, len: usize) -> (usize, usize) {
    let lo = selection.start.min(selection.end).min(len);
    let hi = selection.start.max(selection.end).min(len);
    (lo, hi)
}

fn indent_chars(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(text: &str, edit: &Edit) -> (String, Range<usize>) {
        let mut out = text.to_owned();
        apply(&mut out, edit);
        (out, edit.cursor_after.clone())
    }

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

    #[test]
    fn word_at_finds_identifiers_including_underscores_and_digits() {
        let text = "float set_gain2 = 0;";
        assert_eq!(word_at(text, 8), Some(6..15));
        assert_eq!(word_at(text, 6), Some(6..15));
        assert_eq!(word_at(text, 15), Some(6..15));
    }

    #[test]
    fn word_at_returns_nothing_between_words() {
        let text = "a  b";
        assert_eq!(word_at(text, 2), None);
        assert_eq!(word_at(text, 99), Some(3..4));
    }

    #[test]
    fn word_at_counts_characters_not_bytes() {
        let text = "// héllo world";
        assert_eq!(word_at(text, 4), Some(3..8));
    }

    #[test]
    fn next_occurrence_is_case_sensitive_and_word_bounded() {
        let text = "gain Gain pregain gain_smoothed gain";
        assert_eq!(next_occurrence(text, "gain", 1), Some(32..36));
    }

    #[test]
    fn next_occurrence_wraps_to_the_top() {
        let text = "gain = gain;";
        assert_eq!(next_occurrence(text, "gain", 8), Some(0..4));
    }

    #[test]
    fn next_occurrence_finds_a_hit_at_the_offset_it_starts_from() {
        let text = "a gain b";
        assert_eq!(next_occurrence(text, "gain", 2), Some(2..6));
    }

    #[test]
    fn next_occurrence_skips_word_bounds_for_punctuation_needles() {
        let text = "a->b->c";
        assert_eq!(next_occurrence(text, "->", 2), Some(4..6));
    }

    #[test]
    fn next_occurrence_handles_absent_and_oversized_needles() {
        assert_eq!(next_occurrence("short", "much longer", 0), None);
        assert_eq!(next_occurrence("abc", "", 0), None);
        assert_eq!(next_occurrence("abc", "zzz", 0), None);
    }

    #[test]
    fn finds_every_occurrence_case_insensitively() {
        let text = "float Gain; float gain;";
        assert_eq!(find_matches(text, "gain"), vec![6..10, 18..22]);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(find_matches("anything", "").is_empty());
    }

    #[test]
    fn ranges_are_character_offsets_not_byte_offsets() {
        let text = "é gain";
        assert_eq!(find_matches(text, "gain"), vec![2..6]);
    }

    #[test]
    fn overlapping_scan_does_not_loop_forever() {
        assert_eq!(find_matches("aaaa", "aa"), vec![0..2, 2..4]);
    }

    #[test]
    fn comment_and_uncomment_round_trip_a_single_line() {
        let text = "int a = 1;\n";
        let edit = toggle_comment(text, 4..4, "//").unwrap();
        let (out, cursor) = applied(text, &edit);
        assert_eq!(out, "// int a = 1;\n");
        assert_eq!(cursor, 7..7);

        let back = toggle_comment(&out, 7..7, "//").unwrap();
        let (out, cursor) = applied(&out, &back);
        assert_eq!(out, text);
        assert_eq!(cursor, 4..4);
    }

    #[test]
    fn comment_aligns_on_the_shallowest_indent_and_skips_blank_lines() {
        let text = "  a;\n\n    b;\n";
        let (out, _) = applied(text, &toggle_comment(text, 0..12, "//").unwrap());
        assert_eq!(out, "  // a;\n\n  //   b;\n");
    }

    #[test]
    fn a_partly_commented_block_comments_fully_before_it_uncomments() {
        let text = "// a;\nb;\n";
        let (out, _) = applied(text, &toggle_comment(text, 0..8, "//").unwrap());
        assert_eq!(out, "// // a;\n// b;\n");

        let (back, _) = applied(&out, &toggle_comment(&out, 0..14, "//").unwrap());
        assert_eq!(back, text);
    }

    #[test]
    fn uncomment_tolerates_a_missing_space_after_the_token() {
        let text = "//a;\n//  b;\n";
        let (out, _) = applied(text, &toggle_comment(text, 0..11, "//").unwrap());
        assert_eq!(out, "a;\n b;\n");
    }

    #[test]
    fn toggle_comment_selects_the_whole_result_when_the_selection_was_not_empty() {
        let text = "a;\nb;\n";
        let edit = toggle_comment(text, 0..5, "//").unwrap();
        let (out, cursor) = applied(text, &edit);
        assert_eq!(out, "// a;\n// b;\n");
        assert_eq!(cursor, 0..11);
        assert_eq!(&out[0..11], "// a;\n// b;");
    }

    #[test]
    fn a_caret_before_the_comment_column_does_not_move() {
        let text = "    a;\n";
        let edit = toggle_comment(text, 1..1, "//").unwrap();
        let (out, cursor) = applied(text, &edit);
        assert_eq!(out, "    // a;\n");
        assert_eq!(cursor, 1..1);
    }

    #[test]
    fn toggle_comment_uses_the_language_token() {
        let text = "CFLAGS = -O2\n";
        let (out, _) = applied(text, &toggle_comment(text, 0..0, "#").unwrap());
        assert_eq!(out, "# CFLAGS = -O2\n");
    }

    #[test]
    fn a_blank_span_has_nothing_to_toggle() {
        assert_eq!(toggle_comment("\n\n", 0..2, "//"), None);
        assert_eq!(toggle_comment("", 0..0, "//"), None);
    }

    #[test]
    fn toggle_comment_counts_characters_not_bytes() {
        let text = "// héllo\nx;\n";
        let (out, _) = applied(text, &toggle_comment(text, 0..11, "//").unwrap());
        assert_eq!(out, "// // héllo\n// x;\n");
    }
}
