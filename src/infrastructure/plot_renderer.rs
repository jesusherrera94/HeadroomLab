// The only file that knows about `plotters`. Converts domain `Waveform`/`Spectrum`
// values plus a visible view-range into a `slint::Image`, mirroring the approach
// used by the Slint `plotter` example (SharedPixelBuffer -> BitMapBackend -> Image).
use plotters::coord::types::RangedCoordf32;
use plotters::prelude::*;
use slint::SharedPixelBuffer;

use crate::domain::signal::{Spectrum, Waveform, DB_FLOOR};

const WAVEFORM_COLOR: RGBColor = RGBColor(0x21, 0x96, 0xF3);
const SPECTRUM_COLOR: RGBColor = RGBColor(0xE6, 0x7E, 0x22);
const MIN_FREQ_HZ: f32 = 20.0;

/// Renders the time-domain view of `wave` between `view.0` and `view.1` seconds.
pub fn render_waveform(wave: &Waveform, view: (f32, f32), width: u32, height: u32) -> slint::Image {
    let mut pixel_buffer = SharedPixelBuffer::new(width, height);
    let size = (pixel_buffer.width(), pixel_buffer.height());

    {
        let backend = BitMapBackend::with_buffer(pixel_buffer.make_mut_bytes(), size);
        let root = backend.into_drawing_area();
        root.fill(&WHITE).expect("error filling drawing area");

        if !wave.samples.is_empty() {
            let full_end = wave.duration_seconds().max(1e-3);
            let (start, end) = clamp_range(view, 0.0, full_end);

            let mut chart = ChartBuilder::on(&root)
                .margin(8)
                .x_label_area_size(24)
                .y_label_area_size(36)
                .build_cartesian_2d(start..end, -1.05f32..1.05f32)
                .expect("error building coordinate system");

            chart
                .configure_mesh()
                .x_desc("Time (s)")
                .y_desc("Amplitude")
                .draw()
                .expect("error drawing mesh");

            draw_waveform_series(&mut chart, wave, start, end, size.0 as usize);
        }

        root.present().expect("error presenting drawing area");
    }

    slint::Image::from_rgb8(pixel_buffer)
}

/// Renders the frequency-domain view of `spectrum` between `view_hz.0` and
/// `view_hz.1` Hz, magnitude in dB, frequency on a log scale.
pub fn render_spectrum(spectrum: &Spectrum, view_hz: (f32, f32), width: u32, height: u32) -> slint::Image {
    let mut pixel_buffer = SharedPixelBuffer::new(width, height);
    let size = (pixel_buffer.width(), pixel_buffer.height());

    {
        let backend = BitMapBackend::with_buffer(pixel_buffer.make_mut_bytes(), size);
        let root = backend.into_drawing_area();
        root.fill(&WHITE).expect("error filling drawing area");

        if let Some(&nyquist) = spectrum.frequencies_hz.last() {
            let nyquist = nyquist.max(MIN_FREQ_HZ * 2.0);
            let (start, end) = clamp_range(view_hz, MIN_FREQ_HZ, nyquist);

            let mut chart = ChartBuilder::on(&root)
                .margin(8)
                .x_label_area_size(24)
                .y_label_area_size(36)
                .build_cartesian_2d((start..end).log_scale(), DB_FLOOR..6.0f32)
                .expect("error building coordinate system");

            chart
                .configure_mesh()
                .x_desc("Frequency (Hz)")
                .y_desc("Magnitude (dB)")
                .draw()
                .expect("error drawing mesh");

            draw_spectrum_series(&mut chart, spectrum, start, end, size.0 as usize);
        }

        root.present().expect("error presenting drawing area");
    }

    slint::Image::from_rgb8(pixel_buffer)
}

fn clamp_range(view: (f32, f32), full_min: f32, full_max: f32) -> (f32, f32) {
    let (mut start, mut end) = view;
    if !start.is_finite() || !end.is_finite() || end <= start {
        return (full_min, full_max);
    }
    start = start.clamp(full_min, full_max);
    end = end.clamp(full_min, full_max);
    if end - start < 1e-6 {
        return (full_min, full_max);
    }
    (start, end)
}

type WaveformChart<'a, 'b> = ChartContext<'a, BitMapBackend<'b>, Cartesian2d<RangedCoordf32, RangedCoordf32>>;

/// Draws either every visible sample (when zoomed in enough) or a peak-decimated
/// min/max envelope per pixel column (when the visible range is wide), so
/// zoomed-out views stay fast to rasterize.
fn draw_waveform_series(chart: &mut WaveformChart, wave: &Waveform, start: f32, end: f32, width_px: usize) {
    let sample_rate = wave.sample_rate as f32;
    let start_idx = ((start.max(0.0)) * sample_rate).floor() as usize;
    let end_idx = (((end.max(0.0)) * sample_rate).ceil() as usize).min(wave.samples.len());
    if start_idx >= end_idx {
        return;
    }
    let visible = &wave.samples[start_idx..end_idx];
    let width_px = width_px.max(1);

    if visible.len() <= width_px * 2 {
        let points = visible
            .iter()
            .enumerate()
            .map(|(i, &amp)| ((start_idx + i) as f32 / sample_rate, amp));
        chart
            .draw_series(LineSeries::new(points, WAVEFORM_COLOR))
            .expect("error drawing waveform series");
    } else {
        let bucket = (visible.len() / width_px).max(1);
        let columns = visible.chunks(bucket).enumerate().map(|(i, chunk)| {
            let min = chunk.iter().copied().fold(f32::INFINITY, f32::min);
            let max = chunk.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let t = (start_idx + i * bucket) as f32 / sample_rate;
            (t, min, max)
        });
        chart
            .draw_series(columns.map(|(t, min, max)| PathElement::new(vec![(t, min), (t, max)], WAVEFORM_COLOR)))
            .expect("error drawing waveform envelope");
    }
}

type SpectrumChart<'a, 'b> =
    ChartContext<'a, BitMapBackend<'b>, Cartesian2d<LogCoord<f32>, RangedCoordf32>>;

/// Draws the spectrum, taking the peak dB per pixel column when there are more
/// bins in view than pixels to draw them in.
fn draw_spectrum_series(chart: &mut SpectrumChart, spectrum: &Spectrum, start: f32, end: f32, width_px: usize) {
    let visible_indices: Vec<usize> = spectrum
        .frequencies_hz
        .iter()
        .enumerate()
        .filter(|&(_, &f)| f >= start && f <= end)
        .map(|(i, _)| i)
        .collect();
    if visible_indices.is_empty() {
        return;
    }
    let width_px = width_px.max(1);

    if visible_indices.len() <= width_px * 2 {
        let points = visible_indices
            .iter()
            .map(|&i| (spectrum.frequencies_hz[i], spectrum.magnitudes_db[i]));
        chart
            .draw_series(LineSeries::new(points, SPECTRUM_COLOR))
            .expect("error drawing spectrum series");
    } else {
        let bucket = (visible_indices.len() / width_px).max(1);
        let points: Vec<(f32, f32)> = visible_indices
            .chunks(bucket)
            .map(|chunk| {
                let peak_idx = chunk
                    .iter()
                    .copied()
                    .max_by(|&a, &b| spectrum.magnitudes_db[a].total_cmp(&spectrum.magnitudes_db[b]))
                    .unwrap();
                (spectrum.frequencies_hz[peak_idx], spectrum.magnitudes_db[peak_idx])
            })
            .collect();
        chart
            .draw_series(LineSeries::new(points, SPECTRUM_COLOR))
            .expect("error drawing spectrum envelope");
    }
}
