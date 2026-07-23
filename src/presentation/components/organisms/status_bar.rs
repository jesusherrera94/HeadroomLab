//! The bottom status bar: mocked git branch, cursor position, indentation and
//! encoding, styled like VS Code's. All values are static in HL9.

use eframe::egui::{self, RichText};

use crate::presentation::theme;

pub fn status_bar(ui: &mut egui::Ui, project_name: &str) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        segment(ui, &format!("⑂ main — {project_name}"));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            segment(ui, "UTF-8");
            segment(ui, "Spaces: 2");
            segment(ui, "Ln 1, Col 1");
            segment(ui, "C++");
        });
    });
}

fn segment(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::small_font())
            .color(theme::MUTED_ON_DARK),
    );
    ui.add_space(10.0);
}
