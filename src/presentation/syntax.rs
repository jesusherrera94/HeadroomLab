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

use std::sync::Arc;

use eframe::egui::{self, text::LayoutJob};
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
/// Results are memoized by `egui_extras` on `(theme, text, language)`, so this is
/// safe to call every frame.
pub fn layout_job(
    ctx: &egui::Context,
    style: &egui::Style,
    text: &str,
    lang: Language,
    highlighted: bool,
) -> LayoutJob {
    if !highlighted {
        return plain_job(text);
    }
    highlight(ctx, style, &code_theme(), text, syntect_token(lang))
}

/// Convenience wrapper for use as a `TextEdit::layouter`. The `wrap_width` egui
/// passes is deliberately ignored — see the module docs.
pub fn layouter(ui: &egui::Ui, text: &str, lang: Language, highlighted: bool) -> Arc<egui::Galley> {
    let job = layout_job(ui.ctx(), ui.style(), text, lang, highlighted);
    ui.fonts_mut(|f| f.layout_job(job))
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
    use crate::domain::text_document::{Language, syntect_token};

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
