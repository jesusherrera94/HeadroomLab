use std::ops::Range;
use std::sync::Arc;

use eframe::egui::{
    self,
    text::{ByteIndex, LayoutJob},
};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

use crate::domain::text_document::{Language, syntect_token};
use crate::presentation::theme;

pub fn code_font() -> egui::FontId {
    egui::FontId::monospace(theme::FONT_BODY)
}

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

fn byte_of_char(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn code_theme() -> CodeTheme {
    CodeTheme::dark(theme::FONT_BODY)
}

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
        assert_eq!(
            sections("é gain;", 2..6),
            vec![((0, 3), false), ((3, 7), true), ((7, 8), false)]
        );
    }

    #[test]
    fn a_degenerate_range_changes_nothing() {
        assert_eq!(sections("float gain;", 4..4), vec![((0, 11), false)]);
        assert_eq!(sections("abc", 99..99), vec![((0, 3), false)]);
    }


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

    #[test]
    fn cpp_sources_resolve_to_the_cpp_grammar() {
        let syntaxes = syntect::parsing::SyntaxSet::load_defaults_newlines();
        let syntax = syntaxes
            .find_syntax_by_extension(syntect_token(Language::Cpp))
            .expect("C++ grammar");
        assert_eq!(syntax.name, "C++");
    }
}
