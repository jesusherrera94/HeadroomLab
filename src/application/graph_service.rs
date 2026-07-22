use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::application::ports::{AudioEnginePort, AudioSnapshot};
use crate::domain::audio_processor::AudioProcessor;
use crate::domain::plugin::EffectPlugin;
use crate::domain::signal::{Spectrum, Waveform, compute_spectrum};
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

/// A self-contained, `Send` description of one graph recompute: the audio to
/// analyse plus the full control snapshot to replay onto a fresh plugin
/// instance. Built on the UI thread, executed on the worker thread.
pub struct GraphComputeRequest {
    pub snapshot: AudioSnapshot,
    pub plugin_path: Option<String>,
    pub knob_values: [f32; 6],
    pub switch_positions: [i32; 3],
    pub footswitch_states: [bool; 2],
    /// Bumped whenever a new audio file is loaded, so the worker knows when
    /// its cached original waveform/spectrum are stale.
    pub audio_generation: u64,
}

/// Owns the "what should the processed signal look like right now" question,
/// fully independent from the live playback plugin owned by `SimulatorService`.
///
/// This type only tracks the current control snapshot and a dirty flag on the
/// UI thread; the heavy lifting (plugin render + FFTs) happens in
/// [`compute_graph_data`], which the graph worker runs on a background thread.
pub struct GraphService {
    audio_engine: Rc<dyn AudioEnginePort>,
    plugin_path: RefCell<Option<String>>,
    knob_values: RefCell<[f32; 6]>,
    switch_positions: RefCell<[i32; 3]>, // 1 = MIDDLE default
    footswitch_states: RefCell<[bool; 2]>,
    dirty: Cell<bool>,
    audio_generation: Cell<u64>,
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
            audio_generation: Cell::new(0),
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

    /// Called when a new audio file is loaded, so an already-open graph window
    /// refreshes and the worker invalidates its cached original signal.
    pub fn mark_dirty(&self) {
        self.audio_generation.set(self.audio_generation.get() + 1);
        self.dirty.set(true);
    }

    /// Requests a recompute without invalidating the original-signal cache
    /// (e.g. when the graph window is (re)opened with unchanged audio).
    pub fn mark_params_dirty(&self) {
        self.dirty.set(true);
    }

    /// Returns `true` (and clears the flag) if something changed since the last call.
    pub fn take_dirty(&self) -> bool {
        let was = self.dirty.get();
        self.dirty.set(false);
        was
    }

    /// Packages the current control snapshot + audio buffer into a `Send` job
    /// for the graph worker. Returns `None` only if no audio has been loaded yet.
    pub fn build_request(&self) -> Option<GraphComputeRequest> {
        let snapshot = self.audio_engine.snapshot_samples()?;
        Some(GraphComputeRequest {
            snapshot,
            plugin_path: self.plugin_path.borrow().clone(),
            knob_values: *self.knob_values.borrow(),
            switch_positions: *self.switch_positions.borrow(),
            footswitch_states: *self.footswitch_states.borrow(),
            audio_generation: self.audio_generation.get(),
        })
    }
}

/// Renders the original + processed signals for one request. Pure with respect
/// to shared state, so it can run on any thread (`DylibPlugin` is `Send`).
///
/// `cached_original` lets the caller reuse the original waveform/spectrum
/// across requests with the same `audio_generation`, since turning a knob
/// never changes the original signal.
pub fn compute_graph_data(
    request: &GraphComputeRequest,
    cached_original: Option<(Waveform, Spectrum)>,
) -> GraphData {
    let (original, original_spectrum) = cached_original.unwrap_or_else(|| {
        let original = Waveform::from_interleaved(
            &request.snapshot.samples,
            request.snapshot.channels,
            request.snapshot.sample_rate,
        );
        let spectrum = compute_spectrum(&original);
        (original, spectrum)
    });

    let (processed, processed_spectrum, processed_error) = match render_processed(request) {
        Ok(wave) => {
            let spectrum = compute_spectrum(&wave);
            (Some(wave), Some(spectrum), None)
        }
        Err(msg) => (None, None, Some(msg)),
    };

    GraphData {
        original,
        original_spectrum,
        processed,
        processed_spectrum,
        processed_error,
    }
}

/// Loads a fresh, disposable plugin instance, replays the request's control
/// snapshot onto it, runs it once over the whole original buffer, and lets
/// it drop. No shared state with the live playback plugin.
fn render_processed(request: &GraphComputeRequest) -> Result<Waveform, String> {
    let path = request
        .plugin_path
        .clone()
        .ok_or_else(|| "No effect loaded yet".to_string())?;

    let snapshot = &request.snapshot;
    let mut plugin =
        DylibPlugin::load(&path, snapshot.sample_rate as f32).map_err(|e| e.to_string())?;

    for (index, value) in request.knob_values.iter().enumerate() {
        plugin.set_knob(index, *value);
    }
    for (index, position) in request.switch_positions.iter().enumerate() {
        plugin.set_switch(index, *position);
    }
    for (index, pressed) in request.footswitch_states.iter().enumerate() {
        plugin.set_footswitch(index, *pressed);
    }

    let mut buffer = snapshot.samples.as_ref().clone();
    plugin.process(&mut buffer, snapshot.channels, snapshot.sample_rate);
    Ok(Waveform::from_interleaved(
        &buffer,
        snapshot.channels,
        snapshot.sample_rate,
    ))
}
