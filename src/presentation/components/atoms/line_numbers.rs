//! The code editor's line-number gutter.
//!
//! Numbers are painted from the laid-out galley's own row positions rather than
//! from an assumed row height, so they cannot drift out of alignment with the
//! text no matter what the font metrics do.

use eframe::egui::{self, Align2};

use crate::presentation::syntax::code_font;
use crate::presentation::theme;

/// Horizontal padding either side of the digits.
const PADDING: f32 = 8.0;

/// The gutter width needed for `line_count` lines, at the code font.
pub fn gutter_width(ui: &egui::Ui, line_count: usize) -> f32 {
    let digits = line_count.max(1).to_string().len() as f32;
    let digit_width = ui.fonts_mut(|f| f.glyph_width(&code_font(), '0'));
    digits * digit_width + PADDING * 2.0
}

/// Paints right-aligned line numbers into `rect`, one per galley row, using
/// `galley_top` as the y origin the rows are relative to.
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
