use crate::application::ports::AudioEnginePort;
use crate::domain::audio_track::{AudioError, AudioTrack};
use crate::domain::plugin::{EffectPlugin, PluginError};
use crate::infrastructure::dylib_plugin::{DylibPlugin, SharedPluginProcessor};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

pub struct SimulatorService {
    audio_engine: Rc<dyn AudioEnginePort>,
    current_track: RefCell<Option<AudioTrack>>,
    plugin: RefCell<Option<Arc<Mutex<DylibPlugin>>>>,
}

impl SimulatorService {
    pub fn new(audio_engine: Rc<dyn AudioEnginePort>) -> Self {
        Self {
            audio_engine,
            current_track: RefCell::new(None),
            plugin: RefCell::new(None),
        }
    }

    pub fn load_plugin(&self, path: &str) -> Result<(), PluginError> {
        self.audio_engine.clear_processors();
        let plugin = DylibPlugin::load(path, DEFAULT_SAMPLE_RATE)?;
        let shared = Arc::new(Mutex::new(plugin));
        self.audio_engine
            .add_processor(Box::new(SharedPluginProcessor::new(shared.clone())));
        *self.plugin.borrow_mut() = Some(shared);
        Ok(())
    }

    pub fn unload_plugin(&self) {
        self.audio_engine.clear_processors();
        *self.plugin.borrow_mut() = None;
    }

    pub fn set_knob(&self, index: usize, value: f32) {
        if let Some(p) = self.plugin.borrow().as_ref() {
            if let Ok(mut plugin) = p.lock() {
                plugin.set_knob(index, value);
            }
        }
    }

    pub fn set_switch(&self, index: usize, position: i32) {
        if let Some(p) = self.plugin.borrow().as_ref() {
            if let Ok(mut plugin) = p.lock() {
                plugin.set_switch(index, position);
            }
        }
    }

    pub fn set_footswitch(&self, index: usize, pressed: bool) {
        if let Some(p) = self.plugin.borrow().as_ref() {
            if let Ok(mut plugin) = p.lock() {
                plugin.set_footswitch(index, pressed);
            }
        }
    }

    pub fn handle_file_upload(&self, file_path: String) -> Result<f32, AudioError> {
        let meta = self
            .audio_engine
            .load_file(&file_path)
            .map_err(|_| AudioError::UnsupportedFormat)?; // Simplified mapping

        let track = AudioTrack::validate_and_create(
            file_path,
            meta.duration_seconds,
            meta.sample_rate,
            meta.bit_depth,
            &meta.format,
        )?;

        let duration = track.duration_seconds;
        *self.current_track.borrow_mut() = Some(track);

        Ok(duration)
    }

    pub fn play(&self) {
        self.audio_engine.play();
    }
    pub fn stop(&self) {
        self.audio_engine.stop();
    }
    pub fn toggle_bypass(&self, state: bool) {
        self.audio_engine.set_bypass(state);
    }
    pub fn seek_to(&self, time: f32) {
        self.audio_engine.seek(time);
    }
    pub fn current_position(&self) -> f32 {
        self.audio_engine.current_position()
    }
    pub fn is_playing(&self) -> bool {
        self.audio_engine.is_playing()
    }
}
