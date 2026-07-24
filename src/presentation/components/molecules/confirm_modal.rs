//! Reusable confirmation modal: a titled prompt with Cancel and a confirm
//! button. Only the text and the confirm label change between uses — the caller
//! owns the payload and decides what the confirmation triggers. `destructive`
//! paints the confirm button red (for deletes). Mirrors `error_dialog` styling.

use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::presentation::theme;

/// The text a given confirmation shows.
pub struct ConfirmModalContent<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub confirm_label: &'a str,
    pub destructive: bool,
}

#[derive(Default)]
pub struct ConfirmModalEvents {
    pub confirmed: bool,
    pub cancelled: bool,
}

pub fn confirm_modal(ctx: &egui::Context, content: ConfirmModalContent) -> ConfirmModalEvents {
    let mut events = ConfirmModalEvents::default();

    let accent = if content.destructive {
        theme::ERROR_COLOR
    } else {
        theme::ACCENT
    };

    let frame = egui::Frame::new()
        .fill(theme::DIALOG_BACKGROUND)
        .stroke(Stroke::new(1.0, theme::INSET_BORDER))
        .corner_radius(CornerRadius::same(theme::DIALOG_CORNER_RADIUS))
        .inner_margin(20.0);

    egui::Modal::new(egui::Id::new("confirm_modal"))
        .backdrop_color(theme::MODAL_SCRIM)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.spacing_mut().item_spacing.y = 12.0;

            ui.label(
                RichText::new(content.title)
                    .font(theme::subtitle_font())
                    .strong()
                    .color(theme::LABEL_ON_DARK),
            );
            ui.label(
                RichText::new(content.message)
                    .font(theme::body_font())
                    .color(theme::LABEL_ON_DARK),
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    events.cancelled = true;
                }
                ui.add_space(4.0);
                let confirm = ui.add(
                    egui::Button::new(
                        RichText::new(content.confirm_label).color(egui::Color32::WHITE),
                    )
                    .fill(accent),
                );
                if confirm.clicked() {
                    events.confirmed = true;
                }
            });
        });

    events
}
