//! Toggle Comment: adding or removing a line-comment token across every line
//! a selection touches, keeping the caret where the user left it.

use std::ops::Range;

use super::{Edit, byte_of_char, line_span, slice, sorted};

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

fn indent_chars(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::editing::applied;

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
