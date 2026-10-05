use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::presentation::theme;

pub struct ConfirmModalContent<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub confirm_label: &'a str,
    pub alternate_label: Option<&'a str>,
    pub destructive: bool,
}

#[derive(Default)]
pub struct ConfirmModalEvents {
    pub confirmed: bool,
    pub alternate: bool,
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
                if let Some(label) = content.alternate_label {
                    ui.add_space(4.0);
                    let alternate = ui.add(
                        egui::Button::new(RichText::new(label).color(egui::Color32::WHITE))
                            .fill(theme::ACCENT),
                    );
                    if alternate.clicked() {
                        events.alternate = true;
                    }
                }
            });
        });

    events
}
