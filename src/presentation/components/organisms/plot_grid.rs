use eframe::egui::{self, RichText};

use crate::application::graph_service::GraphData;
use crate::presentation::components::molecules::plot_pane::{spectrum_pane, waveform_pane};
use crate::presentation::theme;

pub struct PaneResets {
    pub orig_time: bool,
    pub orig_freq: bool,
    pub proc_time: bool,
    pub proc_freq: bool,
}

impl Default for PaneResets {
    fn default() -> Self {
        Self {
            orig_time: true,
            orig_freq: true,
            proc_time: true,
            proc_freq: true,
        }
    }
}

impl PaneResets {
    pub fn reset_time_panes(&mut self) {
        self.orig_time = true;
        self.proc_time = true;
    }
}

pub fn plot_grid(
    ui: &mut egui::Ui,
    data: Option<&GraphData>,
    time_full_end: f32,
    resets: &mut PaneResets,
    has_processed: bool,
    processed_status: &str,
) {
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);

    ui.columns(2, |columns| {
        waveform_pane(
            &mut columns[0],
            "orig_time",
            "Original — Time domain",
            data.map(|d| &d.original),
            time_full_end,
            &mut resets.orig_time,
        );
        spectrum_pane(
            &mut columns[1],
            "orig_freq",
            "Original — Frequency domain",
            data.map(|d| &d.original_spectrum),
            &mut resets.orig_freq,
        );
    });

    ui.columns(2, |columns| {
        waveform_pane(
            &mut columns[0],
            "proc_time",
            "Processed — Time domain",
            data.and_then(|d| d.processed.as_ref()),
            time_full_end,
            &mut resets.proc_time,
        );
        spectrum_pane(
            &mut columns[1],
            "proc_freq",
            "Processed — Frequency domain",
            data.and_then(|d| d.processed_spectrum.as_ref()),
            &mut resets.proc_freq,
        );
    });

    if !has_processed {
        let text = if processed_status.is_empty() {
            "No effect loaded — showing original signal only."
        } else {
            processed_status
        };
        ui.label(
            RichText::new(text)
                .font(theme::small_font())
                .color(theme::ERROR_COLOR),
        );
    }
}
