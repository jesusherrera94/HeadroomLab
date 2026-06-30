use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use crate::application::graph_service::{GraphData, GraphService};
use crate::domain::signal::{Spectrum, Waveform};
use crate::infrastructure::plot_renderer::{render_spectrum, render_waveform};
use crate::GraphWindow;

const PLOT_WIDTH_PX: u32 = 480;
const PLOT_HEIGHT_PX: u32 = 300;

/// Wires a `GraphWindow` to a `GraphService`: registers the four `render_*`
/// pure callbacks (rasterizing via `plot_renderer`) and starts a 100ms polling
/// timer — the same idiom `simulation_controller` uses — that recomputes the
/// signals only when `GraphService` reports something changed.
pub fn bind(ui: &GraphWindow, graph_service: Rc<GraphService>) -> Timer {
    let cached: Rc<RefCell<Option<GraphData>>> = Rc::new(RefCell::new(graph_service.compute_signals()));
    let last_full_end: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
    apply_metadata(ui, cached.borrow().as_ref(), &last_full_end);

    ui.on_render_original_time({
        let cached = cached.clone();
        move |start, end, _version| match cached.borrow().as_ref() {
            Some(data) => render_waveform(&data.original, (start, end), PLOT_WIDTH_PX, PLOT_HEIGHT_PX),
            None => blank_waveform(),
        }
    });

    ui.on_render_original_freq({
        let cached = cached.clone();
        move |start, end, _version| match cached.borrow().as_ref() {
            Some(data) => render_spectrum(&data.original_spectrum, (start, end), PLOT_WIDTH_PX, PLOT_HEIGHT_PX),
            None => blank_spectrum(),
        }
    });

    ui.on_render_processed_time({
        let cached = cached.clone();
        move |start, end, _version| match cached.borrow().as_ref().and_then(|d| d.processed.as_ref()) {
            Some(wave) => render_waveform(wave, (start, end), PLOT_WIDTH_PX, PLOT_HEIGHT_PX),
            None => blank_waveform(),
        }
    });

    ui.on_render_processed_freq({
        let cached = cached.clone();
        move |start, end, _version| match cached.borrow().as_ref().and_then(|d| d.processed_spectrum.as_ref()) {
            Some(spectrum) => render_spectrum(spectrum, (start, end), PLOT_WIDTH_PX, PLOT_HEIGHT_PX),
            None => blank_spectrum(),
        }
    });

    let ui_handle = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(ui) = ui_handle.upgrade() else { return };
        if !graph_service.take_dirty() {
            return;
        }
        let data = graph_service.compute_signals();
        apply_metadata(&ui, data.as_ref(), &last_full_end);
        *cached.borrow_mut() = data;
        ui.set_data_version(ui.get_data_version() + 1);
    });
    timer
}

/// Updates the non-plot UI state (status text, full-range bounds) and resets
/// the visible view to the full range only when the track's duration actually
/// changes (e.g. a new file was loaded) — turning a knob must never yank the
/// user's current zoom/pan back to the full view.
fn apply_metadata(ui: &GraphWindow, data: Option<&GraphData>, last_full_end: &Cell<f32>) {
    match data {
        Some(d) => {
            ui.set_has_processed(d.processed.is_some());
            ui.set_processed_status(d.processed_error.clone().unwrap_or_default().into());

            let full_end = d.original.duration_seconds().max(1e-3);
            ui.set_orig_time_full_end(full_end);
            if (full_end - last_full_end.get()).abs() > 1e-6 {
                last_full_end.set(full_end);
                ui.set_orig_time_start(0.0);
                ui.set_orig_time_end(full_end);
                ui.set_proc_time_start(0.0);
                ui.set_proc_time_end(full_end);
            }
        }
        None => {
            ui.set_has_processed(false);
            ui.set_processed_status("No audio loaded yet.".into());
        }
    }
}

fn blank_waveform() -> slint::Image {
    render_waveform(
        &Waveform { samples: Vec::new(), sample_rate: 48_000 },
        (0.0, 1.0),
        PLOT_WIDTH_PX,
        PLOT_HEIGHT_PX,
    )
}

fn blank_spectrum() -> slint::Image {
    render_spectrum(
        &Spectrum { frequencies_hz: Vec::new(), magnitudes_db: Vec::new() },
        (20.0, 24_000.0),
        PLOT_WIDTH_PX,
        PLOT_HEIGHT_PX,
    )
}
