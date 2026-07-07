//! Signal graph window: the 2x2 Original/Processed x Time/Frequency plot grid.

use eframe::egui::{self, RichText};

use crate::presentation::components::organisms::plot_grid::plot_grid;
use crate::presentation::graph_controller::GraphSession;
use crate::presentation::theme;

pub fn show(ui: &mut egui::Ui, session: &mut GraphSession) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.label(RichText::new("Signal Graph").font(theme::title_font()).strong());

        let time_full_end = session.time_full_end();
        let has_processed = session.has_processed;
        let status = session.processed_status.clone();
        plot_grid(
            ui,
            session.cached.as_ref(),
            time_full_end,
            &mut session.resets,
            has_processed,
            &status,
        );
    });
}
