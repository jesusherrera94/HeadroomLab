use eframe::egui;

use crate::presentation::theme;

/// A 3-step toggle switch (UP / MID / DN). `position`: 0 = UP, 1 = MIDDLE,
/// 2 = DOWN. Returns `true` when the position changed this frame.
pub fn three_way_switch(ui: &mut egui::Ui, label: &str, position: &mut i32) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .font(theme::small_font())
                .color(theme::LABEL_ON_DARK),
        );
        ui.horizontal(|ui| {
            for (step, text) in [(0, "UP"), (1, "MID"), (2, "DN")] {
                let selected = *position == step;
                if ui.add(theme::selectable_button(text, selected)).clicked() && !selected {
                    *position = step;
                    changed = true;
                }
            }
        });
    });
    changed
}
