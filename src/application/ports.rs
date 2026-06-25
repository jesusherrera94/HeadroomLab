use crate::domain::counter::Counter;
// Port: the application depends on this abstraction.

pub struct AudioMetadata {
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub format: String,
}

pub trait CounterRepository {
    fn load(&self) -> Counter;
    fn save(&self, counter: &Counter);
}

pub trait AudioEnginePort {
    fn load_file(&self, path: &str) -> Result<AudioMetadata, String>;
    fn play(&self);
    fn stop(&self);
    fn set_bypass(&self, enabled: bool);
    fn seek(&self, time_seconds: f32);
    fn add_processor(&self, processor: Box<dyn crate::domain::audio_processor::AudioProcessor>);
    fn clear_processors(&self);
}