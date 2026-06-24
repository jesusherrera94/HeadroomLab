use crate::domain::counter::Counter;
use crate::domain::audio_track::AudioTrack;
// Port: the application depends on this abstraction.

pub trait CounterRepository {
    fn load(&self) -> Counter;
    fn save(&self, counter: &Counter);
}

pub trait AudioEnginePort {
    fn load_file(&self, path: &str) -> Result<(f32, u32, u8, String), String>;
    fn play(&self);
    fn stop(&self);
    fn set_bypass(&self, enabled: bool);
    fn seek(&self, time_seconds: f32);
}