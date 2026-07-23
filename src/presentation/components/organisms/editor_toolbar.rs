//! The editor toolbar: three actions that drive the C++ build/run cycle.
//! "Open emulator" is live in HL9; "Build & run" and "Compile" are wired to
//! the terminal in a later task (Day 11).

use eframe::egui::{self, RichText};

use crate::presentation::theme;

#[derive(Default)]
pub struct EditorToolbarEvents {
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
}

pub fn editor_toolbar(ui: &mut egui::Ui) -> EditorToolbarEvents {
    let mut events = EditorToolbarEvents::default();

    ui.horizontal(|ui| {
        ui.add_space(4.0);
        if ui
            .add(theme::selectable_button("Open emulator", true))
            .clicked()
        {
            events.open_emulator = true;
        }
        if ui
            .add(theme::selectable_button("Build & run emulator", false))
            .clicked()
        {
            events.build_run = true;
        }
        if ui.add(theme::selectable_button("Compile", false)).clicked() {
            events.compile = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new("HeadroomLab")
                    .font(theme::small_font())
                    .color(theme::MUTED_ON_DARK),
            );
        });
    });

    events
}
