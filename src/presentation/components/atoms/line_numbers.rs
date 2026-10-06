
use eframe::egui::{self, Align2};
use crate::presentation::syntax::code_font;
use crate::presentation::theme;

const PADDING: f32 = 8.0;

pub fn gutter_width(ui: &egui::Ui, line_count: usize) -> f32 {
    let digits = line_count.max(1).to_string().len() as f32;
    let digit_width = ui.fonts_mut(|f| f.glyph_width(&code_font(), '0'));
    digits * digit_width + PADDING * 2.0
}

pub fn line_numbers(ui: &egui::Ui, rect: egui::Rect, galley: &egui::Galley, galley_top: f32) {
    let painter = ui.painter();
    let x = rect.right() - PADDING;

    for (index, row) in galley.rows.iter().enumerate() {
        let y = galley_top + row.min_y();
        painter.text(
            egui::pos2(x, y),
            Align2::RIGHT_TOP,
            (index + 1).to_string(),
            code_font(),
            theme::MUTED_ON_DARK,
        );
    }
}
