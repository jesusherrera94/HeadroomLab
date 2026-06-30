// Pure signal-processing types and math: no IO, no Slint, no plugin knowledge.
// Reusable for any feature that needs to look at a buffer of audio samples.
use std::f32::consts::PI;

use rustfft::{num_complex::Complex32, FftPlanner};

/// A mono signal, normalised to [-1.0, 1.0].
pub struct Waveform {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Waveform {
    /// Downmixes interleaved multi-channel samples to mono by averaging channels.
    pub fn from_interleaved(samples: &[f32], channels: u16, sample_rate: u32) -> Self {
        let channels = channels.max(1) as usize;
        let mono = samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect();
        Self { samples: mono, sample_rate }
    }

    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

/// A magnitude spectrum: parallel arrays of frequency (Hz) and magnitude (dB).
pub struct Spectrum {
    pub frequencies_hz: Vec<f32>,
    pub magnitudes_db: Vec<f32>,
}

/// Display floor so silence/empty bins don't produce `-inf` dB.
pub const DB_FLOOR: f32 = -80.0;

/// Hann-windowed, single-shot FFT magnitude spectrum (0 Hz .. Nyquist).
///
/// This is a static "frequency response" snapshot of the whole buffer, not a
/// sliding STFT — sufficient for comparing an effect's tonal shape at the
/// current control settings.
pub fn compute_spectrum(waveform: &Waveform) -> Spectrum {
    let n = waveform.samples.len();
    if n == 0 || waveform.sample_rate == 0 {
        return Spectrum { frequencies_hz: vec![], magnitudes_db: vec![] };
    }

    // 1. Apply a Hann window to reduce spectral leakage.
    let mut buffer: Vec<Complex32> = waveform
        .samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = if n > 1 {
                0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos()
            } else {
                1.0
            };
            Complex32::new(s * w, 0.0)
        })
        .collect();

    // 2. Single forward FFT. rustfft handles non-power-of-two sizes via
    //    mixed-radix/Bluestein, which is fine here since this only runs on the
    //    debounced ~10 Hz graph timer, never on the audio thread.
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buffer);

    // 3. Convert bins [0, n/2] to magnitude, then dB, clamped to DB_FLOOR.
    let half = n / 2;
    let mut frequencies_hz = Vec::with_capacity(half + 1);
    let mut magnitudes_db = Vec::with_capacity(half + 1);
    for (k, bin) in buffer.iter().take(half + 1).enumerate() {
        // Bin 0 (DC) and the Nyquist bin are not doubled; all others represent
        // both the positive and the mirrored negative frequency.
        let scale = if k == 0 || k == half { 1.0 } else { 2.0 };
        let magnitude = bin.norm() * scale / n as f32;
        let db = (20.0 * magnitude.max(1e-6).log10()).max(DB_FLOOR);

        frequencies_hz.push(k as f32 * waveform.sample_rate as f32 / n as f32);
        magnitudes_db.push(db);
    }

    Spectrum { frequencies_hz, magnitudes_db }
}
