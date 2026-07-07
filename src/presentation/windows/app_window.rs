//! Root application window: effect build path picker + simulator launcher.

use eframe::egui;

#[derive(Default)]
pub struct AppWindowEvents {
    pub browse_clicked: bool,
    pub launch_clicked: bool,
}

pub fn show(ui: &mut egui::Ui, effect_build_path: &mut String) -> AppWindowEvents {
    let mut events = AppWindowEvents::default();

    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            let browse_width = 80.0;
            let edit = egui::TextEdit::singleline(effect_build_path)
                .hint_text("Path to effect .dylib…")
                .desired_width(ui.available_width() - browse_width);
            ui.add(edit);
            if ui.button("Browse…").clicked() {
                events.browse_clicked = true;
            }
        });

        if ui.button("Launch simulator").clicked() {
            events.launch_clicked = true;
        }
    });

    events
}
