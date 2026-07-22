//! Mocked splash / updater screen: logo, title, an indeterminate progress bar
//! and a "Checking for updates…" line. No interaction — the app controller
//! auto-advances to the Initial window after ~1s.

use eframe::egui::{self, CornerRadius, RichText};

use crate::presentation::components::atoms::logo;
use crate::presentation::theme;

pub fn show(ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            logo::logo_tile(ui, 56.0);
            ui.add_space(14.0);
            ui.label(
                RichText::new("Headroom Lab")
                    .font(theme::subtitle_font())
                    .color(theme::LABEL_ON_DARK),
            );
            ui.add_space(14.0);
            progress_bar(ui);
            ui.add_space(8.0);
            ui.label(
                RichText::new("Checking for updates…")
                    .font(theme::body_font())
                    .color(theme::MUTED_ON_DARK),
            );
            ui.add_space(24.0);
            ui.label(
                RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                    .font(egui::FontId::monospace(theme::FONT_SMALL))
                    .color(theme::MUTED_ON_DARK),
            );
        });
    });
}

/// A static, partially-filled progress track (mock — no real updater).
fn progress_bar(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(180.0, 4.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), theme::INSET_SURFACE);
    let mut fill = rect;
    fill.set_width(rect.width() * 0.38);
    painter.rect_filled(fill, CornerRadius::same(2), theme::LOGO_STROKE);
}
