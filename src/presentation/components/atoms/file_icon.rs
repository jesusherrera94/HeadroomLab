//! A tiny per-file-kind icon glyph, painted as a short colored text token in a
//! fixed-width slot so explorer rows and tabs align. No icon font yet — that
//! arrives with the real file explorer (Day 5); this keeps the layout readable.

use eframe::egui::{self, Color32};

use crate::presentation::editor_controller::FileKind;
use crate::presentation::theme;

/// Width reserved for the icon so names line up across rows.
const ICON_WIDTH: f32 = 22.0;

/// Draws the icon for `kind`, allocating a fixed-width slot in the layout.
pub fn file_icon(ui: &mut egui::Ui, kind: FileKind) {
    let (glyph, color) = glyph_and_color(kind);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ICON_WIDTH, ui.text_style_height(&egui::TextStyle::Body)),
        egui::Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::monospace(theme::FONT_SMALL),
            color,
        );
    }
}

fn glyph_and_color(kind: FileKind) -> (&'static str, Color32) {
    match kind {
        FileKind::Folder => ("▸", theme::LABEL_ON_DARK),
        FileKind::Cpp => ("C+", theme::WAVEFORM_COLOR),
        FileKind::Header => (".h", theme::SPECTRUM_COLOR),
        FileKind::Make => ("M", theme::LOGO_STROKE),
        FileKind::Markdown => ("md", theme::MUTED_ON_DARK),
        FileKind::Other => ("•", theme::MUTED_ON_DARK),
    }
}
