use std::path::Path;
use crate::application::ports::AudioEnginePort;

pub struct AudioEngine;

impl AudioEngine {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEnginePort for AudioEngine {
    fn load_file(&self, path: &str) -> Result<(f32, u32, u8, String), String> {
        println!("[AudioEngine] Loading file: {}", path);
        
        // Extract the extension to pass the domain validation
        let extension = Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("wav")
            .to_string();

        // Return mock data that satisfies the Daisy Seed acceptance criteria:
        // Duration: 10.5 seconds (under 15s limit)
        // Sample Rate: 48,000 Hz
        // Bit Depth: 16 bit
        Ok((10.5, 48000, 16, extension))
    }

    fn play(&self) {
        println!("[AudioEngine] Playback started.");
    }

    fn stop(&self) {
        println!("[AudioEngine] Playback stopped.");
    }

    fn set_bypass(&self, enabled: bool) {
        if enabled {
            println!("[AudioEngine] Effect BYPASSED (Dry signal only).");
        } else {
            println!("[AudioEngine] Effect ACTIVE (Wet signal).");
        }
    }

    fn seek(&self, time_seconds: f32) {
        println!("[AudioEngine] Seeked to {:.2} seconds.", time_seconds);
    }
}