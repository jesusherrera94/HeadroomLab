//! The app mark: a "play" triangle flanked by short input/output leads and a
//! little waveform, painted directly (no image asset — the prototype PNG is
//! gitignored and not shippable). Mirrors the prototype SVG on a 64×64 grid.

use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};

use crate::presentation::theme;

/// Draws the mark centred in a square of side `size`, allocating that space in
/// the current layout. `stroke_color` lets callers tint it (defaults to the
/// logo blue via [`logo`]).
pub fn logo_colored(ui: &mut egui::Ui, size: f32, stroke_color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter();
    // Map the prototype's 0..64 viewBox onto `rect`.
    let s = size / 64.0;
    let p = |x: f32, y: f32| Pos2::new(rect.left() + x * s, rect.top() + y * s);
    let wide = Stroke::new(3.0 * s, stroke_color);
    let thin = Stroke::new(2.0 * s, stroke_color);

    // Play triangle.
    let tri = [p(18.0, 12.0), p(52.0, 32.0), p(18.0, 52.0)];
    painter.add(egui::Shape::closed_line(tri.to_vec(), wide));

    // Input/output leads.
    painter.line_segment([p(6.0, 23.0), p(18.0, 23.0)], thin);
    painter.line_segment([p(6.0, 41.0), p(18.0, 41.0)], thin);
    painter.line_segment([p(52.0, 32.0), p(60.0, 32.0)], thin);

    // Little waveform across the triangle.
    let wave = [
        p(23.0, 40.0),
        p(26.0, 26.0),
        p(29.0, 34.0),
        p(32.0, 28.0),
        p(35.0, 35.0),
        p(38.0, 31.0),
        p(41.0, 34.0),
        p(44.0, 32.0),
    ];
    painter.add(egui::Shape::line(wave.to_vec(), thin));

    response
}

/// Draws the mark tinted with the app's logo blue.
pub fn logo(ui: &mut egui::Ui, size: f32) -> egui::Response {
    logo_colored(ui, size, theme::LOGO_STROKE)
}

/// Draws the mark inside a rounded inset tile of side `tile`, as in the splash
/// and initial-window headers. The mark is sized to ~64% of the tile.
pub fn logo_tile(ui: &mut egui::Ui, tile: f32) -> egui::Response {
    let corner = egui::CornerRadius::same((tile * 0.25) as u8);
    egui::Frame::new()
        .fill(theme::INSET_SURFACE)
        .stroke(Stroke::new(1.0, theme::INSET_BORDER))
        .corner_radius(corner)
        .inner_margin(tile * 0.18)
        .show(ui, |ui| logo(ui, tile * 0.64))
        .inner
}
