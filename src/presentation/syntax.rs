//! The code-area text layouter. The **only** module that knows about syntect —
//! everything else asks for a galley and gets one.
//!
//! Two decisions are baked in here:
//!
//! * The syntect theme contributes token **foreground** colours only. The
//!   background stays the app's panel fill, so the code area reads as continuous
//!   with the active tab (see `theme::TAB_ACTIVE_BACKGROUND`). `egui_extras`
//!   cooperates: its syntect path sets `color`/`italics`/`underline` on each
//!   section and never a background.
//! * Wrapping is **off**. A `LayoutJob`'s default wrap width is infinite, which
//!   is what we want: long lines scroll horizontally instead of reflowing, so a
//!   source line always occupies exactly one galley row and the line-number
//!   gutter can align to it.

use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{
    self,
    text::{ByteIndex, LayoutJob},
};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

use crate::domain::text_document::{Language, syntect_token};
use crate::presentation::theme;

/// The monospace font the code area and its gutter share.
pub fn code_font() -> egui::FontId {
    egui::FontId::monospace(theme::FONT_BODY)
}

/// Builds the layout job for `text`.
///
/// `highlighted` comes from the document classification: oversized buffers stay
/// editable but render as plain monospace, because syntect re-highlights the
/// whole buffer on every keystroke.
///
/// `match_range` is a **character** range to paint with the find-match
/// background. It is drawn into the layout rather than left to the `TextEdit`'s
/// selection because egui paints a selection only while that widget has focus
/// (`text_edit/builder.rs:833`) — and while the find bar is open, focus is in
/// the query field, so a selected match would be invisible.
///
/// Results are memoized by `egui_extras` on `(theme, text, language)`, so this is
/// safe to call every frame.
pub fn layout_job(
    ctx: &egui::Context,
    style: &egui::Style,
    text: &str,
    lang: Language,
    highlighted: bool,
    match_range: Option<&Range<usize>>,
) -> LayoutJob {
    let mut job = if highlighted {
        highlight(ctx, style, &code_theme(), text, syntect_token(lang))
    } else {
        plain_job(text)
    };
    if let Some(range) = match_range {
        paint_match(&mut job, text, range);
    }
    job
}

/// Convenience wrapper for use as a `TextEdit::layouter`. The `wrap_width` egui
/// passes is deliberately ignored — see the module docs.
pub fn layouter(
    ui: &egui::Ui,
    text: &str,
    lang: Language,
    highlighted: bool,
    match_range: Option<&Range<usize>>,
) -> Arc<egui::Galley> {
    let job = layout_job(ui.ctx(), ui.style(), text, lang, highlighted, match_range);
    ui.fonts_mut(|f| f.layout_job(job))
}

/// Gives every section overlapping `range` the find-match background, splitting
/// the sections it straddles so the tint stops exactly at the match.
///
/// `range` is in characters (what `find_matches` and egui's cursors speak);
/// sections index by byte, so it is converted once on the way in.
fn paint_match(job: &mut LayoutJob, text: &str, range: &Range<usize>) {
    let start = byte_of_char(text, range.start);
    let end = byte_of_char(text, range.end);
    if start >= end {
        return;
    }

    let mut sections = Vec::with_capacity(job.sections.len() + 2);
    for section in job.sections.drain(..) {
        let (from, to) = (section.byte_range.start.0, section.byte_range.end.0);
        if to <= start || from >= end {
            sections.push(section);
            continue;
        }

        if from < start {
            let mut head = section.clone();
            head.byte_range = ByteIndex(from)..ByteIndex(start);
            sections.push(head);
        }

        let mut hit = section.clone();
        hit.byte_range = ByteIndex(from.max(start))..ByteIndex(to.min(end));
        hit.format.background = theme::FIND_MATCH;
        sections.push(hit);

        if to > end {
            let mut tail = section.clone();
            tail.byte_range = ByteIndex(end)..ByteIndex(to);
            sections.push(tail);
        }
    }
    job.sections = sections;
}

/// Byte offset of character offset `index`, clamping past the end.
fn byte_of_char(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// The dark code theme, at the app's monospace size.
///
/// `egui_extras` keeps its `SyntectTheme` enum private and hardcodes
/// base16-mocha.dark for `dark()`, so the theme itself is not selectable through
/// the public API. That only costs us the exact palette — the background, which
/// is what actually had to match the app, is ours either way.
fn code_theme() -> CodeTheme {
    CodeTheme::dark(theme::FONT_BODY)
}

/// Unhighlighted fallback: one section, monospace, primary label colour.
fn plain_job(text: &str) -> LayoutJob {
    LayoutJob::simple(
        text.to_owned(),
        code_font(),
        theme::LABEL_ON_DARK,
        f32::INFINITY,
    )
}

#[cfg(test)]
mod tests {
    use super::{paint_match, plain_job};
    use crate::domain::text_document::{Language, syntect_token};
    use crate::presentation::theme;

    /// Each section as `(byte_range, is_highlighted)`.
    fn sections(text: &str, range: std::ops::Range<usize>) -> Vec<((usize, usize), bool)> {
        let mut job = plain_job(text);
        paint_match(&mut job, text, &range);
        job.sections
            .iter()
            .map(|s| {
                (
                    (s.byte_range.start.0, s.byte_range.end.0),
                    s.format.background == theme::FIND_MATCH,
                )
            })
            .collect()
    }

    #[test]
    fn a_match_inside_a_section_splits_it_in_three() {
        // "float gain;" — highlight `gain` at chars 6..10.
        assert_eq!(
            sections("float gain;", 6..10),
            vec![((0, 6), false), ((6, 10), true), ((10, 11), false)]
        );
    }

    #[test]
    fn a_match_at_either_edge_splits_it_in_two() {
        assert_eq!(
            sections("gain = 1;", 0..4),
            vec![((0, 4), true), ((4, 9), false)]
        );
        assert_eq!(
            sections("x = gain", 4..8),
            vec![((0, 4), false), ((4, 8), true)]
        );
    }

    #[test]
    fn a_match_covering_everything_leaves_one_highlighted_section() {
        assert_eq!(sections("gain", 0..4), vec![((0, 4), true)]);
    }

    #[test]
    fn character_offsets_are_converted_to_byte_offsets() {
        // "é" is two bytes, so `gain` sits at chars 2..6 but bytes 3..7.
        assert_eq!(
            sections("é gain;", 2..6),
            vec![((0, 3), false), ((3, 7), true), ((7, 8), false)]
        );
    }

    #[test]
    fn a_degenerate_range_changes_nothing() {
        assert_eq!(sections("float gain;", 4..4), vec![((0, 11), false)]);
        // Past the end clamps to the end, which is then empty.
        assert_eq!(sections("abc", 99..99), vec![((0, 3), false)]);
    }

    /// Every token we hand `egui_extras` must resolve to a real grammar.
    ///
    /// This is the contract that would otherwise fail *silently*: an unknown
    /// token makes `highlight_impl` return `None` and the text renders in flat
    /// grey, which looks like a styling bug rather than a lookup miss. The
    /// lookup mirrors what `egui_extras` does internally — by name, then by
    /// file extension.
    #[test]
    fn every_language_token_resolves_to_a_syntect_grammar() {
        let syntaxes = syntect::parsing::SyntaxSet::load_defaults_newlines();

        for lang in [
            Language::Cpp,
            Language::C,
            Language::Header,
            Language::Make,
            Language::Markdown,
            Language::Json,
            Language::PlainText,
        ] {
            let token = syntect_token(lang);
            let found = syntaxes
                .find_syntax_by_name(token)
                .or_else(|| syntaxes.find_syntax_by_extension(token));
            assert!(
                found.is_some(),
                "no syntect grammar for {lang:?} (token {token:?})"
            );
        }
    }

    /// The C++ grammar is the one that actually matters for this app.
    #[test]
    fn cpp_sources_resolve_to_the_cpp_grammar() {
        let syntaxes = syntect::parsing::SyntaxSet::load_defaults_newlines();
        let syntax = syntaxes
            .find_syntax_by_extension(syntect_token(Language::Cpp))
            .expect("C++ grammar");
        assert_eq!(syntax.name, "C++");
    }
}
