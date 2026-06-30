use crate::domain::counter::Counter;
use crate::domain::plugin::PluginError;
// Port: the application depends on this abstraction.

pub struct AudioMetadata {
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub format: String,
}

/// A point-in-time copy of the original (unprocessed) decoded audio buffer,
/// used to render the graph view without touching the live playback state.
pub struct AudioSnapshot {
    pub samples: Vec<f32>, // interleaved, original/unprocessed
    pub sample_rate: u32,
    pub channels: u16,
}

pub trait CounterRepository {
    fn load(&self) -> Counter;
    fn save(&self, counter: &Counter);
}

pub trait PluginLoaderPort {
    /// Load the effect from `path`. Replaces any previously loaded plugin.
    fn load_plugin(&self, path: &str) -> Result<(), PluginError>;
    fn unload_plugin(&self);
}

pub trait AudioEnginePort {
    fn load_file(&self, path: &str) -> Result<AudioMetadata, String>;
    fn play(&self);
    fn stop(&self);
    fn set_bypass(&self, enabled: bool);
    fn seek(&self, time_seconds: f32);
    fn add_processor(&self, processor: Box<dyn crate::domain::audio_processor::AudioProcessor>);
    fn clear_processors(&self);
    fn current_position(&self) -> f32;
    fn is_playing(&self) -> bool;
    /// Returns a copy of the original decoded buffer, or `None` if no audio is loaded.
    fn snapshot_samples(&self) -> Option<AudioSnapshot>;
}