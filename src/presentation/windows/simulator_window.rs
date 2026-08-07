//! Hardware simulator window: audio player transport, simulated hardware
//! controls and the modal error dialog.

use eframe::egui::{self, RichText};

use crate::domain::menu;
use crate::presentation::components::molecules::error_dialog::error_dialog;
use crate::presentation::components::molecules::transport_bar::{TransportEvents, transport_bar};
use crate::presentation::components::organisms::hardware_controls_panel::{
    HardwareEvents, hardware_controls_panel,
};
use crate::presentation::menu_controller;
use crate::presentation::simulation_controller::SimulatorState;
use crate::presentation::theme;

#[derive(Default)]
pub struct SimulatorViewEvents {
    pub transport: TransportEvents,
    pub hardware: HardwareEvents,
    pub error_dismissed: bool,
}

pub fn show(ui: &mut egui::Ui, state: &mut SimulatorState) -> SimulatorViewEvents {
    let mut events = SimulatorViewEvents::default();

    // Space toggles playback, as the Transport menu advertises. Bound only in
    // this window, so it never reaches the code editor as a keystroke.
    if state.has_audio {
        let play_pause = menu_controller::shortcut(menu::PLAY_PAUSE);
        events.transport.play_toggled = ui.input_mut(|i| i.consume_shortcut(&play_pause));
    }

    egui::CentralPanel::default().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 15.0;

        ui.label(
            RichText::new("Audio Player")
                .font(theme::title_font())
                .strong(),
        );

        events.transport = transport_bar(
            ui,
            state.has_audio,
            state.is_playing,
            state.is_bypassed,
            &mut state.current_time,
            state.duration,
        );

        events.hardware = hardware_controls_panel(ui, &mut state.hardware);
    });

    if state.show_error && error_dialog(&ui.ctx().clone(), &state.error_message) {
        state.show_error = false;
        events.error_dismissed = true;
    }

    events
}
