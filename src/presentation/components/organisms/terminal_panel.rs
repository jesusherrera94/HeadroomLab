//! The built-in terminal panel: a "TERMINAL" header and a mocked scrollback of
//! monospace lines plus a prompt caret. The real PTY-backed shell arrives in
//! Day 10.

use eframe::egui::{self, RichText};

use crate::presentation::theme;

pub fn terminal_panel(ui: &mut egui::Ui, lines: &[String]) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("TERMINAL")
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
        );
    });
    ui.add_space(2.0);

    egui::Frame::new()
        .fill(theme::PLOT_FRAME_BACKGROUND)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in lines {
                        ui.label(
                            RichText::new(line)
                                .font(egui::FontId::monospace(theme::FONT_BODY))
                                .color(theme::LABEL_ON_DARK),
                        );
                    }
                    ui.label(
                        RichText::new("$ ▏")
                            .font(egui::FontId::monospace(theme::FONT_BODY))
                            .color(theme::MUTED_ON_DARK),
                    );
                });
        });
}
