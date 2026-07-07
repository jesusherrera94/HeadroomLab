//! One quadrant of the graph grid: a titled interactive plot with
//! scroll-to-zoom, drag-to-pan (x-axis only, like the previous version) and a
//! "Reset view" button. Time panes plot seconds linearly; frequency panes plot
//! log10(Hz) with axis labels formatted back to Hz.

use eframe::egui;
use egui_plot::{AxisHints, HoverPosition, Line, Plot, PlotBounds, PlotPoints, PlotUi};

use crate::domain::signal::{Spectrum, Waveform, DB_FLOOR};
use crate::presentation::theme;

const PLOT_HEIGHT_PX: f32 = 300.0;
/// Point budget per pane: beyond this the series is peak-decimated.
const POINT_BUDGET: usize = 960;
/// Minimum zoom span as a fraction of the full range (parity with the previous UI).
const MIN_SPAN_FRACTION: f64 = 1.0 / 200.0;

pub const MIN_FREQ_HZ: f64 = 20.0;
pub const MAX_FREQ_HZ: f64 = 24_000.0;

/// Renders the time-domain view of `wave`. `full_end` is the track duration in
/// seconds; `reset` forces the view back to the full range this frame.
pub fn waveform_pane(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    wave: Option<&Waveform>,
    full_end: f32,
    reset: &mut bool,
) {
    let full_range = (0.0f64, full_end.max(1e-3) as f64);
    let x_axis = AxisHints::new_x().label("Time (s)");
    let y_axis = AxisHints::new_y().label("Amplitude");

    pane_chrome(ui, title, |ui| {
        let plot = base_plot(id)
            .custom_x_axes(vec![x_axis])
            .custom_y_axes(vec![y_axis]);
        plot.show(ui, |plot_ui| {
            let bounds = tame_bounds(plot_ui, full_range, (-1.05, 1.05), reset);
            if let Some(wave) = wave {
                let points = waveform_points(wave, bounds.min()[0], bounds.max()[0]);
                plot_ui.line(
                    Line::new("waveform", PlotPoints::from(points)).color(theme::WAVEFORM_COLOR),
                );
            }
        });
    });

    reset_button(ui, reset);
}

/// Renders the frequency-domain view of `spectrum` on a log-frequency axis.
pub fn spectrum_pane(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    spectrum: Option<&Spectrum>,
    reset: &mut bool,
) {
    let full_range = (MIN_FREQ_HZ.log10(), MAX_FREQ_HZ.log10());
    let x_axis = AxisHints::new_x()
        .label("Frequency (Hz)")
        .formatter(|mark, _range| format_hz(10f64.powf(mark.value)));
    let y_axis = AxisHints::new_y().label("Magnitude (dB)");

    pane_chrome(ui, title, |ui| {
        let plot = base_plot(id)
            .custom_x_axes(vec![x_axis])
            .custom_y_axes(vec![y_axis])
            .label_formatter(|hover| {
                let position = match hover {
                    HoverPosition::NearDataPoint { position, .. }
                    | HoverPosition::Elsewhere { position } => position,
                };
                Some(format!(
                    "{}\n{:.1} dB",
                    format_hz(10f64.powf(position.x)),
                    position.y
                ))
            });
        plot.show(ui, |plot_ui| {
            let bounds = tame_bounds(plot_ui, full_range, (DB_FLOOR as f64, 6.0), reset);
            if let Some(spectrum) = spectrum {
                let points = spectrum_points(spectrum, bounds.min()[0], bounds.max()[0]);
                plot_ui.line(
                    Line::new("spectrum", PlotPoints::from(points)).color(theme::SPECTRUM_COLOR),
                );
            }
        });
    });

    reset_button(ui, reset);
}

// -- Shared pane behavior -----------------------------------------------------

fn pane_chrome(ui: &mut egui::Ui, title: &str, add_plot: impl FnOnce(&mut egui::Ui)) {
    ui.label(
        egui::RichText::new(title)
            .font(theme::body_font())
            .color(theme::LABEL_ON_DARK),
    );
    egui::Frame::new()
        .fill(theme::PLOT_BACKGROUND)
        .stroke(egui::Stroke::new(1.0, theme::PLOT_FRAME_BORDER))
        .show(ui, add_plot);
}

fn base_plot(id: &str) -> Plot<'static> {
    Plot::new(id.to_owned())
        .height(PLOT_HEIGHT_PX)
        .allow_drag([true, false]) // pan on x only, like the previous drag behavior
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
}

fn reset_button(ui: &mut egui::Ui, reset: &mut bool) {
    if ui.button("Reset view").clicked() {
        *reset = true;
        ui.ctx().request_repaint();
    }
}

/// Applies the pane's interaction rules and returns the final bounds:
/// - `reset` snaps x back to the full range,
/// - plain scroll zooms x around the view center (0.9x / 1.1x per event),
/// - x span is clamped to 1/200 of the full span,
/// - y is pinned to the fixed range (only x is interactive).
fn tame_bounds(
    plot_ui: &mut PlotUi,
    full_x: (f64, f64),
    fixed_y: (f64, f64),
    reset: &mut bool,
) -> PlotBounds {
    let mut bounds = plot_ui.plot_bounds();
    let (mut start, mut end) = (bounds.min()[0], bounds.max()[0]);

    if *reset || !start.is_finite() || !end.is_finite() || end <= start {
        (start, end) = full_x;
        *reset = false;
    }

    let full_span = full_x.1 - full_x.0;
    let min_span = full_span * MIN_SPAN_FRACTION;

    if plot_ui.response().hovered() {
        let scroll = plot_ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let factor = if scroll > 0.0 { 0.9 } else { 1.1 };
            let span = ((end - start) * factor).max(min_span);
            let center = (start + end) / 2.0;
            start = center - span / 2.0;
            end = center + span / 2.0;
        }
    }

    if end - start < min_span {
        let center = (start + end) / 2.0;
        start = center - min_span / 2.0;
        end = center + min_span / 2.0;
    }

    bounds = PlotBounds::from_min_max([start, fixed_y.0], [end, fixed_y.1]);
    plot_ui.set_plot_bounds(bounds);
    bounds
}

fn format_hz(hz: f64) -> String {
    if hz >= 1000.0 {
        format!("{:.1}k", hz / 1000.0)
    } else {
        format!("{hz:.0}")
    }
}

// -- Series construction (visible range only, peak-decimated when wide) --------

/// Either every visible sample (when zoomed in enough) or a min/max envelope
/// per bucket (when the visible range is wide), so zoomed-out views stay fast.
fn waveform_points(wave: &Waveform, start_s: f64, end_s: f64) -> Vec<[f64; 2]> {
    let sample_rate = wave.sample_rate as f64;
    if wave.samples.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }
    let start_idx = (start_s.max(0.0) * sample_rate).floor() as usize;
    let end_idx = ((end_s.max(0.0) * sample_rate).ceil() as usize).min(wave.samples.len());
    if start_idx >= end_idx {
        return Vec::new();
    }
    let visible = &wave.samples[start_idx..end_idx];

    if visible.len() <= POINT_BUDGET * 2 {
        visible
            .iter()
            .enumerate()
            .map(|(i, &amp)| [(start_idx + i) as f64 / sample_rate, amp as f64])
            .collect()
    } else {
        let bucket = (visible.len() / POINT_BUDGET).max(1);
        let mut points = Vec::with_capacity((visible.len() / bucket + 1) * 2);
        for (i, chunk) in visible.chunks(bucket).enumerate() {
            let min = chunk.iter().copied().fold(f32::INFINITY, f32::min);
            let max = chunk.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let t = (start_idx + i * bucket) as f64 / sample_rate;
            // Vertical min->max strokes per bucket, connected into one line.
            points.push([t, min as f64]);
            points.push([t, max as f64]);
        }
        points
    }
}

/// Spectrum points in (log10 Hz, dB) space; peak dB per bucket when there are
/// more visible bins than the point budget.
fn spectrum_points(spectrum: &Spectrum, start_log: f64, end_log: f64) -> Vec<[f64; 2]> {
    let visible: Vec<usize> = spectrum
        .frequencies_hz
        .iter()
        .enumerate()
        .filter(|&(_, &f)| {
            let f = f as f64;
            f >= MIN_FREQ_HZ && f.log10() >= start_log && f.log10() <= end_log
        })
        .map(|(i, _)| i)
        .collect();
    if visible.is_empty() {
        return Vec::new();
    }

    let to_point = |i: usize| {
        [
            (spectrum.frequencies_hz[i] as f64).log10(),
            spectrum.magnitudes_db[i] as f64,
        ]
    };

    if visible.len() <= POINT_BUDGET * 2 {
        visible.iter().map(|&i| to_point(i)).collect()
    } else {
        let bucket = (visible.len() / POINT_BUDGET).max(1);
        visible
            .chunks(bucket)
            .map(|chunk| {
                let peak = chunk
                    .iter()
                    .copied()
                    .max_by(|&a, &b| {
                        spectrum.magnitudes_db[a].total_cmp(&spectrum.magnitudes_db[b])
                    })
                    .unwrap();
                to_point(peak)
            })
            .collect()
    }
}
