//! A per-node icon glyph from the Phosphor icon font, painted in a fixed-width
//! slot so explorer rows and tabs align. `NodeIcon::Generic` is the fallback for
//! unknown file kinds, so every entry always gets an icon.

use eframe::egui::{self, Color32};
use egui_phosphor::regular as ph;

use crate::presentation::editor_controller::NodeIcon;
use crate::presentation::theme;

/// Width reserved for the icon so names line up across rows.
const ICON_WIDTH: f32 = 20.0;
/// Rendered glyph size.
const ICON_SIZE: f32 = 16.0;

/// Draws the glyph for `icon`, allocating a fixed-width slot in the layout.
pub fn file_icon(ui: &mut egui::Ui, icon: NodeIcon) {
    let (glyph, color) = glyph_and_color(icon);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ICON_WIDTH, ui.text_style_height(&egui::TextStyle::Body)),
        egui::Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(ICON_SIZE),
            color,
        );
    }
}

fn glyph_and_color(icon: NodeIcon) -> (&'static str, Color32) {
    match icon {
        NodeIcon::FolderClosed => (ph::FOLDER, theme::LOGO_STROKE),
        NodeIcon::FolderOpen => (ph::FOLDER_OPEN, theme::LOGO_STROKE),
        NodeIcon::Cpp => (ph::FILE_CPP, theme::WAVEFORM_COLOR),
        NodeIcon::Header => (ph::FILE_C, theme::SPECTRUM_COLOR),
        NodeIcon::Build => (ph::WRENCH, theme::LABEL_ON_DARK),
        NodeIcon::Markdown => (ph::FILE_MD, theme::LABEL_ON_DARK),
        NodeIcon::Json => (ph::BRACKETS_CURLY, theme::MUTED_ON_DARK),
        NodeIcon::Text => (ph::FILE_TEXT, theme::MUTED_ON_DARK),
        NodeIcon::Audio => (ph::FILE_AUDIO, theme::SPECTRUM_COLOR),
        NodeIcon::Library => (ph::STACK, theme::MUTED_ON_DARK),
        NodeIcon::Git => (ph::GIT_BRANCH, theme::ERROR_COLOR),
        NodeIcon::Generic => (ph::FILE, theme::MUTED_ON_DARK),
    }
}
