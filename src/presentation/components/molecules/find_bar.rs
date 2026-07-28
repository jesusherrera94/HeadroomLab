//! In-buffer find bar (Cmd/Ctrl+F): query field, match count and next/prev
//! stepping. Matches are shown by selecting them in the code editor rather than
//! by tinting the layout job — the selection highlight egui already draws is the
//! same affordance, and it keeps the syntax layouter untouched.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::presentation::theme;

/// Live find state for the active buffer.
pub struct FindState {
    pub query: String,
    /// Which match is current, as an index into the match list.
    pub current: usize,
    /// Set on open so the field grabs focus once.
    pub focus: bool,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            query: String::new(),
            current: 0,
            focus: true,
        }
    }
}

#[derive(Default)]
pub struct FindEvents {
    pub changed: bool,
    pub next: bool,
    pub previous: bool,
    pub close: bool,
}

pub fn find_bar(ui: &mut egui::Ui, state: &mut FindState, match_count: usize) -> FindEvents {
    let mut events = FindEvents::default();

    egui::Frame::new()
        .fill(theme::INSET_SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::INSET_BORDER))
        .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(ph::MAGNIFYING_GLASS)
                        .font(egui::FontId::proportional(theme::FONT_BODY))
                        .color(theme::MUTED_ON_DARK),
                );

                let field = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .desired_width(200.0)
                        .hint_text("Find"),
                );
                if state.focus {
                    field.request_focus();
                    state.focus = false;
                }
                if field.changed() {
                    events.changed = true;
                }
                // Enter steps to the next hit, the way every editor's find works.
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    events.next = true;
                    field.request_focus();
                }

                let summary = if state.query.is_empty() {
                    String::new()
                } else if match_count == 0 {
                    "No results".to_owned()
                } else {
                    format!("{} of {match_count}", state.current + 1)
                };
                ui.label(
                    RichText::new(summary)
                        .font(theme::small_font())
                        .color(theme::MUTED_ON_DARK),
                );

                let enabled = match_count > 0;
                if ui
                    .add_enabled(enabled, theme::selectable_button(ph::CARET_UP, false))
                    .clicked()
                {
                    events.previous = true;
                }
                if ui
                    .add_enabled(enabled, theme::selectable_button(ph::CARET_DOWN, false))
                    .clicked()
                {
                    events.next = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(theme::selectable_button(ph::X, false)).clicked() {
                        events.close = true;
                    }
                });
            });
        });

    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        events.close = true;
    }

    events
}

/// Character ranges of every case-insensitive occurrence of `query` in `text`.
///
/// Ranges are in **characters**, not bytes, because that is what egui's cursors
/// use. Returns nothing for an empty query.
pub fn find_matches(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let haystack = text.to_lowercase();
    let needle = query.to_lowercase();

    // Byte offset → char offset, so the ranges line up with the cursor model.
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

#[cfg(test)]
mod tests {
    use super::find_matches;

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
        // "é" is two bytes; the match after it must still report char offsets.
        let text = "é gain";
        assert_eq!(find_matches(text, "gain"), vec![2..6]);
    }

    #[test]
    fn overlapping_scan_does_not_loop_forever() {
        assert_eq!(find_matches("aaaa", "aa"), vec![0..2, 2..4]);
    }
}
