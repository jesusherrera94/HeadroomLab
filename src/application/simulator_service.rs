use std::cell::RefCell;
use std::rc::Rc;
use crate::domain::audio_track::{AudioTrack, AudioError};
use crate::application::ports::AudioEnginePort;

pub struct SimulatorService {
    audio_engine: Rc<dyn AudioEnginePort>,
    current_track: RefCell<Option<AudioTrack>>,
}

impl SimulatorService {
    pub fn new(audio_engine: Rc<dyn AudioEnginePort>) -> Self {
        Self {
            audio_engine,
            current_track: RefCell::new(None),
        }
    }

    pub fn handle_file_upload(&self, file_path: String) -> Result<f32, AudioError> {
        // e.g., /Users/jesusherrera/development/HeadroomLab/test_samples/guitar_riff.wav
        
        let meta = self.audio_engine.load_file(&file_path)
            .map_err(|_| AudioError::UnsupportedFormat)?; // Simplified mapping
            
        let track = AudioTrack::validate_and_create(
            file_path, meta.duration_seconds, meta.sample_rate, meta.bit_depth, &meta.format
        )?;
        
        let duration = track.duration_seconds;
        *self.current_track.borrow_mut() = Some(track);
        
        Ok(duration)
    }

    pub fn play(&self) { self.audio_engine.play(); }
    pub fn stop(&self) { self.audio_engine.stop(); }
    pub fn toggle_bypass(&self, state: bool) { self.audio_engine.set_bypass(state); }
    pub fn seek_to(&self, time: f32) { self.audio_engine.seek(time); }
}
