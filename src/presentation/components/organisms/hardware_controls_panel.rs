//! Simulated Hothouse hardware controls: 6 knobs, 3 three-way switches and
//! 2 footswitches on the dark hardware panel.

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

use crate::presentation::components::atoms::{
    footswitch::footswitch, knob::knob, three_way_switch::three_way_switch,
};
use crate::presentation::theme;

/// Control changes emitted this frame as `(index, value)` pairs.
#[derive(Default)]
pub struct HardwareEvents {
    pub knob_changes: Vec<(usize, f32)>,
    pub switch_changes: Vec<(usize, i32)>,
    pub footswitch_changes: Vec<(usize, bool)>,
}

pub struct HardwareControlsState {
    pub knob_values: [f32; 6],
    pub switch_positions: [i32; 3],
    pub footswitch_states: [bool; 2],
}

impl Default for HardwareControlsState {
    fn default() -> Self {
        Self {
            knob_values: [0.0; 6],
            switch_positions: [1; 3], // MIDDLE
            footswitch_states: [false; 2],
        }
    }
}

pub fn hardware_controls_panel(
    ui: &mut egui::Ui,
    state: &mut HardwareControlsState,
) -> HardwareEvents {
    let mut events = HardwareEvents::default();

    egui::Frame::new()
        .fill(theme::PANEL_BACKGROUND)
        .stroke(Stroke::new(1.0, theme::PANEL_BORDER))
        .corner_radius(CornerRadius::same(theme::CORNER_RADIUS))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;

            ui.label(
                RichText::new("Hardware Controls")
                    .font(theme::subtitle_font())
                    .strong()
                    .color(Color32::WHITE),
            );

            section_label(ui, "Knobs");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let knob_width = ((ui.available_width() - 8.0 * 5.0) / 6.0).clamp(40.0, 160.0);
                ui.spacing_mut().slider_width = knob_width;
                for (index, value) in state.knob_values.iter_mut().enumerate() {
                    if knob(ui, &format!("Knob {}", index + 1), value) {
                        events.knob_changes.push((index, *value));
                    }
                }
            });

            section_label(ui, "Switches");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (index, position) in state.switch_positions.iter_mut().enumerate() {
                    if three_way_switch(ui, &format!("SW {}", index + 1), position) {
                        events.switch_changes.push((index, *position));
                    }
                }
            });

            section_label(ui, "Footswitches");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (index, pressed) in state.footswitch_states.iter_mut().enumerate() {
                    if footswitch(ui, &format!("FS {}", index + 1), pressed) {
                        events.footswitch_changes.push((index, *pressed));
                    }
                }
            });
        });

    events
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::small_font())
            .color(theme::MUTED_ON_DARK),
    );
}
