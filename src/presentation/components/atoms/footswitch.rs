use eframe::egui;

use crate::presentation::theme;

/// A footswitch toggle button labeled "<label> [ON]/[OFF]". Returns `true`
/// when toggled this frame.
pub fn footswitch(ui: &mut egui::Ui, label: &str, pressed: &mut bool) -> bool {
    let text = if *pressed {
        format!("{label} [ON]")
    } else {
        format!("{label} [OFF]")
    };
    if ui.add(theme::selectable_button(&text, *pressed)).clicked() {
        *pressed = !*pressed;
        return true;
    }
    false
}
