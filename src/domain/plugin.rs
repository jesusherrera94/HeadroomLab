#[derive(Debug)]
pub enum PluginError {
    FileNotFound(String),
    LoadFailed(String),
    InvalidPlugin(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::FileNotFound(p) => write!(f, "No build found at: {p}"),
            PluginError::LoadFailed(msg) => write!(f, "Failed to load plugin: {msg}"),
            PluginError::InvalidPlugin(msg) => write!(f, "Invalid plugin: {msg}"),
        }
    }
}

/// Control interface from the HAL adapter to the loaded effect.
pub trait EffectPlugin: Send {
    fn set_knob(&mut self, index: usize, value: f32);
    fn set_switch(&mut self, index: usize, position: i32);
    fn set_footswitch(&mut self, index: usize, pressed: bool);
}
