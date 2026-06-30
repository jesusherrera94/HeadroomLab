use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::application::ports::{AudioEnginePort, AudioSnapshot};
use crate::domain::audio_processor::AudioProcessor;
use crate::domain::plugin::EffectPlugin;
use crate::domain::signal::{compute_spectrum, Spectrum, Waveform};
use crate::infrastructure::dylib_plugin::DylibPlugin;

/// Everything the graph window needs to redraw all four plots.
pub struct GraphData {
    pub original: Waveform,
    pub original_spectrum: Spectrum,
    /// `None` when no effect is loaded, or the dedicated render instance failed to load.
    pub processed: Option<Waveform>,
    pub processed_spectrum: Option<Spectrum>,
    /// Human-readable reason `processed` is `None`, surfaced as inline status text.
    pub processed_error: Option<String>,
}

/// Owns the "what should the processed signal look like right now" question,
/// fully independent from the live playback plugin owned by `SimulatorService`.
///
/// Every recompute loads a *fresh, disposable* plugin instance, replays the
/// current control snapshot onto it, and renders the whole original buffer
/// once. This never touches the realtime audio thread.
pub struct GraphService {
    audio_engine: Rc<dyn AudioEnginePort>,
    plugin_path: RefCell<Option<String>>,
    knob_values: RefCell<[f32; 6]>,
    switch_positions: RefCell<[i32; 3]>, // 1 = MIDDLE default
    footswitch_states: RefCell<[bool; 2]>,
    dirty: Cell<bool>,
}

impl GraphService {
    pub fn new(audio_engine: Rc<dyn AudioEnginePort>) -> Self {
        Self {
            audio_engine,
            plugin_path: RefCell::new(None),
            knob_values: RefCell::new([0.0; 6]),
            switch_positions: RefCell::new([1; 3]),
            footswitch_states: RefCell::new([false; 2]),
            dirty: Cell::new(true),
        }
    }

    pub fn set_plugin_path(&self, path: &str) {
        *self.plugin_path.borrow_mut() = Some(path.to_string());
        self.dirty.set(true);
    }

    pub fn set_knob(&self, index: usize, value: f32) {
        if let Some(slot) = self.knob_values.borrow_mut().get_mut(index) {
            *slot = value;
        }
        self.dirty.set(true);
    }

    pub fn set_switch(&self, index: usize, position: i32) {
        if let Some(slot) = self.switch_positions.borrow_mut().get_mut(index) {
            *slot = position;
        }
        self.dirty.set(true);
    }

    pub fn set_footswitch(&self, index: usize, pressed: bool) {
        if let Some(slot) = self.footswitch_states.borrow_mut().get_mut(index) {
            *slot = pressed;
        }
        self.dirty.set(true);
    }

    /// Called when a new audio file is loaded, so an already-open graph window refreshes.
    pub fn mark_dirty(&self) {
        self.dirty.set(true);
    }

    /// Returns `true` (and clears the flag) if something changed since the last call.
    pub fn take_dirty(&self) -> bool {
        let was = self.dirty.get();
        self.dirty.set(false);
        was
    }

    /// Renders the original + processed signals using the *current* control
    /// snapshot. Returns `None` only if no audio has been loaded yet.
    pub fn compute_signals(&self) -> Option<GraphData> {
        let snapshot = self.audio_engine.snapshot_samples()?;
        let original = Waveform::from_interleaved(&snapshot.samples, snapshot.channels, snapshot.sample_rate);
        let original_spectrum = compute_spectrum(&original);

        let (processed, processed_spectrum, processed_error) = match self.render_processed(&snapshot) {
            Ok(wave) => {
                let spectrum = compute_spectrum(&wave);
                (Some(wave), Some(spectrum), None)
            }
            Err(msg) => (None, None, Some(msg)),
        };

        Some(GraphData {
            original,
            original_spectrum,
            processed,
            processed_spectrum,
            processed_error,
        })
    }

    /// Loads a fresh, disposable plugin instance, replays the current control
    /// snapshot onto it, runs it once over the whole original buffer, and lets
    /// it drop. No shared state with the live playback plugin.
    fn render_processed(&self, snapshot: &AudioSnapshot) -> Result<Waveform, String> {
        let path = self
            .plugin_path
            .borrow()
            .clone()
            .ok_or_else(|| "No effect loaded yet".to_string())?;

        let mut plugin = DylibPlugin::load(&path, snapshot.sample_rate as f32).map_err(|e| e.to_string())?;

        for (index, value) in self.knob_values.borrow().iter().enumerate() {
            plugin.set_knob(index, *value);
        }
        for (index, position) in self.switch_positions.borrow().iter().enumerate() {
            plugin.set_switch(index, *position);
        }
        for (index, pressed) in self.footswitch_states.borrow().iter().enumerate() {
            plugin.set_footswitch(index, *pressed);
        }

        let mut buffer = snapshot.samples.clone();
        plugin.process(&mut buffer, snapshot.channels, snapshot.sample_rate);
        Ok(Waveform::from_interleaved(&buffer, snapshot.channels, snapshot.sample_rate))
    }
}
