use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::presentation::theme;

/// Modal error dialog: dark scrim that swallows clicks behind it, centered
/// red-bordered panel with an "Error" title, the message, and a Dismiss
/// button. Returns `true` when dismissed this frame.
pub fn error_dialog(ctx: &egui::Context, message: &str) -> bool {
    let mut dismissed = false;

    let frame = egui::Frame::new()
        .fill(theme::DIALOG_BACKGROUND)
        .stroke(Stroke::new(2.0, theme::ERROR_COLOR))
        .corner_radius(CornerRadius::same(theme::DIALOG_CORNER_RADIUS))
        .inner_margin(20.0);

    egui::Modal::new(egui::Id::new("error_dialog"))
        .backdrop_color(theme::MODAL_SCRIM)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.spacing_mut().item_spacing.y = 16.0;

            ui.label(
                RichText::new("Error")
                    .font(theme::subtitle_font())
                    .strong()
                    .color(theme::ERROR_COLOR),
            );
            ui.label(
                RichText::new(message)
                    .font(theme::body_font())
                    .color(theme::LABEL_ON_DARK),
            );
            if ui.button("Dismiss").clicked() {
                dismissed = true;
            }
        });

    dismissed
}
