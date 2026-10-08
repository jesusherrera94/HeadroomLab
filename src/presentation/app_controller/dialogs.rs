//! Modal dialogs shown over the app's windows.

use eframe::egui;

use crate::presentation::theme;

pub(super) fn notice_dialog(ctx: &egui::Context, text: &str, dismissable: bool) -> bool {
    let mut dismissed = false;
    egui::Modal::new(egui::Id::new("update_notice")).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            ui.label(RichTextExt::body(text));
            ui.add_space(14.0);
            if ui
                .add_enabled(dismissable, egui::Button::new("OK"))
                .clicked()
            {
                dismissed = true;
            }
            ui.add_space(4.0);
        });
    });
    dismissed
}

struct RichTextExt;

impl RichTextExt {
    fn body(text: &str) -> egui::RichText {
        egui::RichText::new(text).font(theme::body_font())
    }
}

pub(super) fn about_dialog(ctx: &egui::Context) -> bool {
    let mut dismissed = false;
    egui::Modal::new(egui::Id::new("about_headroomlab")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("HeadroomLab")
                    .font(theme::title_font())
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .font(theme::small_font())
                    .color(theme::MUTED_ON_DARK),
            );
            ui.add_space(10.0);
            ui.label("Audition guitar-pedal DSP effects on real audio.");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Effects are C-ABI shared libraries exporting hl_create, hl_process, \
                     hl_set_knob, hl_set_switch, hl_set_footswitch and hl_destroy.",
                )
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
            );
            ui.add_space(14.0);
            if ui.button("OK").clicked() {
                dismissed = true;
            }
            ui.add_space(4.0);
        });
    });
    dismissed
}
