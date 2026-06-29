#[derive(Debug, PartialEq)]
pub enum AudioError {
    ExceedsMaxLength,
    UnsupportedFormat,
    InvalidSampleRate,
    InvalidBitDepth,
}

pub struct AudioTrack {
    pub file_path: String,
    pub duration_seconds: f32,
}

impl AudioTrack {
    pub fn validate_and_create(
        file_path: String,
        duration_seconds: f32,
        sample_rate: u32,
        bit_depth: u8,
        extension: &str,
    ) -> Result<Self, AudioError> {
        if duration_seconds > 15.0 {
            return Err(AudioError::ExceedsMaxLength);
        }
        if sample_rate != 48000 {
            return Err(AudioError::InvalidSampleRate);
        }
        if bit_depth != 16 && bit_depth != 32 {
            return Err(AudioError::InvalidBitDepth);
        }
        if !["wav", "mp3", "ogg"].contains(&extension.to_lowercase().as_str()) {
            return Err(AudioError::UnsupportedFormat);
        }

        Ok(Self { file_path, duration_seconds })
    }
}
