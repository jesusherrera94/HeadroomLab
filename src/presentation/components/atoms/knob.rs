use eframe::egui;

use crate::presentation::theme;

/// A single knob: label + 0..1 slider. Returns `true` when the value changed
/// this frame (the caller emits `(index, value)` to the services).
pub fn knob(ui: &mut egui::Ui, label: &str, value: &mut f32) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .font(theme::small_font())
                .color(theme::LABEL_ON_DARK),
        );
        let slider = egui::Slider::new(value, 0.0..=1.0).show_value(false);
        changed = ui.add(slider).changed();
    });
    changed
}
