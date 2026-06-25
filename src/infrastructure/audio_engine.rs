use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crate::application::ports::{AudioEnginePort, AudioMetadata};
use crate::domain::audio_processor::AudioProcessor;
use crate::infrastructure::audio_decoder::decode_audio_file;


// ── Shared state (audio thread + main thread) ─────────────────────────────────
struct PlaybackState {
    samples: Vec<f32>,         // decoded PCM, interleaved, normalised to [-1, 1]
    sample_rate: u32,
    channels: u16,
    playhead: usize,           // current sample index (interleaved)
    is_playing: bool,
    is_bypassed: bool,
    processors: Vec<Box<dyn AudioProcessor>>,
}
impl PlaybackState {
    fn empty() -> Self {
        Self {
            samples: Vec::new(),
            sample_rate: 48_000,
            channels: 2,
            playhead: 0,
            is_playing: false,
            is_bypassed: false,
            processors: Vec::new(),
        }
    }
}


pub struct AudioEngine {
    state: Arc<Mutex<PlaybackState>>,
    stream: RefCell<Option<Stream>>, // main-thread only
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PlaybackState::empty())),
            stream: RefCell::new(None),
        }
    }

    fn rebuild_stream(&self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("No output device available")?;
        let config: StreamConfig = {
            let state = self.state.lock().unwrap();
            StreamConfig {
                channels: state.channels,
                sample_rate: cpal::SampleRate(state.sample_rate),
                buffer_size: cpal::BufferSize::Default,
            }
        };

        let state_arc = Arc::clone(&self.state);
        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _| {
                    fill_output(output, &state_arc);
                },
                |err| eprintln!("[AudioEngine] stream error: {err}"),
                None,
            )
            .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;
        *self.stream.borrow_mut() = Some(stream);
        Ok(())
    }
}


fn fill_output(output: &mut [f32], state_arc: &Arc<Mutex<PlaybackState>>) {
    let mut state = match state_arc.try_lock() {
        Ok(s) => s,
        Err(_) => {
            // Lock contention: output silence rather than block or glitch
            output.fill(0.0);
            return;
        }
    };
    if !state.is_playing || state.samples.is_empty() {
        output.fill(0.0);
        return;
    }
    for frame in output.chunks_mut(state.channels as usize) {
        if state.playhead + frame.len() > state.samples.len() {
            // Reached end of audio
            frame.fill(0.0);
            state.is_playing = false;
            break;
        }
        frame.copy_from_slice(&state.samples[state.playhead..state.playhead + frame.len()]);
        state.playhead += frame.len();
    }
    // Apply effect chain (skipped when bypassed)
    if !state.is_bypassed {
        let (sample_rate, channels) = (state.sample_rate, state.channels);
        for processor in &mut state.processors {
            processor.process(output, channels, sample_rate);
        }
    }
}

impl AudioEnginePort for AudioEngine {
    fn load_file(&self, path: &str) -> Result<AudioMetadata, String> {
        let (samples, sample_rate, channels, duration_seconds, format) =
            decode_audio_file(path)?;
        {
            let mut state = self.state.lock().unwrap();
            state.samples = samples;
            state.sample_rate = sample_rate;
            state.channels = channels;
            state.playhead = 0;
            state.is_playing = false;
        }
        self.rebuild_stream()?;
        Ok(AudioMetadata {
            duration_seconds,
            sample_rate,
            bit_depth: 32, // we always decode to f32
            format,
        })
    }

    fn play(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_playing = true;
    }

    fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_playing = false;
        state.playhead = 0;
    }

    fn set_bypass(&self, enabled: bool) {
        self.state.lock().unwrap().is_bypassed = enabled;
    }

    fn seek(&self, time_seconds: f32) {
        let mut state = self.state.lock().unwrap();
        let sample_pos = (time_seconds * state.sample_rate as f32) as usize
            * state.channels as usize;
        state.playhead = sample_pos.min(state.samples.len());
    }

    fn add_processor(&self, processor: Box<dyn AudioProcessor>) {
        self.state.lock().unwrap().processors.push(processor);
    }

    fn clear_processors(&self) {
        self.state.lock().unwrap().processors.clear();
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}
