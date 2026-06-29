use std::sync::{Arc, Mutex};

use libloading::{Library, Symbol};

use crate::domain::audio_processor::AudioProcessor;
use crate::domain::plugin::{EffectPlugin, PluginError};

// FFI function signatures exported by every effect .dylib.
type FnCreate = unsafe extern "C" fn(f32) -> *mut std::ffi::c_void;
type FnDestroy = unsafe extern "C" fn(*mut std::ffi::c_void);
type FnProcess = unsafe extern "C" fn(*mut std::ffi::c_void, *mut f32, usize, u16, u32);
type FnSetKnob = unsafe extern "C" fn(*mut std::ffi::c_void, u32, f32);
type FnSetSwitch = unsafe extern "C" fn(*mut std::ffi::c_void, u32, i32);
type FnSetFootswitch = unsafe extern "C" fn(*mut std::ffi::c_void, u32, bool);

pub struct DylibPlugin {
    _library: Library, // kept alive for the lifetime of the plugin
    instance: *mut std::ffi::c_void,
    fn_destroy: FnDestroy,
    fn_process: FnProcess,
    fn_set_knob: FnSetKnob,
    fn_set_switch: FnSetSwitch,
    fn_set_fs: FnSetFootswitch,
}

impl DylibPlugin {
    pub fn load(path: &str, sample_rate: f32) -> Result<Self, PluginError> {
        if !std::path::Path::new(path).exists() {
            return Err(PluginError::FileNotFound(path.to_string()));
        }

        unsafe {
            let library =
                Library::new(path).map_err(|e| PluginError::LoadFailed(e.to_string()))?;

            let fn_create: FnCreate = load_symbol(&library, b"hl_create\0", "hl_create")?;
            let fn_destroy: FnDestroy = load_symbol(&library, b"hl_destroy\0", "hl_destroy")?;
            let fn_process: FnProcess = load_symbol(&library, b"hl_process\0", "hl_process")?;
            let fn_set_knob: FnSetKnob = load_symbol(&library, b"hl_set_knob\0", "hl_set_knob")?;
            let fn_set_switch: FnSetSwitch =
                load_symbol(&library, b"hl_set_switch\0", "hl_set_switch")?;
            let fn_set_fs: FnSetFootswitch =
                load_symbol(&library, b"hl_set_footswitch\0", "hl_set_footswitch")?;

            let instance = fn_create(sample_rate);
            if instance.is_null() {
                return Err(PluginError::InvalidPlugin(
                    "hl_create returned a null instance".to_string(),
                ));
            }

            Ok(Self {
                _library: library,
                instance,
                fn_destroy,
                fn_process,
                fn_set_knob,
                fn_set_switch,
                fn_set_fs,
            })
        }
    }
}

/// Resolves a single required symbol, mapping a missing symbol to `InvalidPlugin`.
unsafe fn load_symbol<T: Copy>(
    library: &Library,
    name: &[u8],
    display: &str,
) -> Result<T, PluginError> {
    let symbol: Symbol<T> = unsafe {
        library
            .get(name)
            .map_err(|_| PluginError::InvalidPlugin(format!("missing symbol {display}")))?
    };
    Ok(*symbol)
}

impl Drop for DylibPlugin {
    fn drop(&mut self) {
        unsafe { (self.fn_destroy)(self.instance) };
    }
}

// The raw pointer is an opaque handle owned exclusively by this struct.
unsafe impl Send for DylibPlugin {}

impl EffectPlugin for DylibPlugin {
    fn set_knob(&mut self, index: usize, value: f32) {
        unsafe { (self.fn_set_knob)(self.instance, index as u32, value) }
    }
    fn set_switch(&mut self, index: usize, position: i32) {
        unsafe { (self.fn_set_switch)(self.instance, index as u32, position) }
    }
    fn set_footswitch(&mut self, index: usize, pressed: bool) {
        unsafe { (self.fn_set_fs)(self.instance, index as u32, pressed) }
    }
}

impl AudioProcessor for DylibPlugin {
    fn process(&mut self, samples: &mut [f32], channels: u16, sample_rate: u32) {
        unsafe {
            (self.fn_process)(
                self.instance,
                samples.as_mut_ptr(),
                samples.len(),
                channels,
                sample_rate,
            )
        }
    }
    fn reset(&mut self) {}
}

/// Audio-thread shim: shares a single loaded plugin between the control
/// (UI) thread and the audio callback via `Arc<Mutex<…>>`.
pub struct SharedPluginProcessor {
    inner: Arc<Mutex<DylibPlugin>>,
}

impl SharedPluginProcessor {
    pub fn new(inner: Arc<Mutex<DylibPlugin>>) -> Self {
        Self { inner }
    }
}

impl AudioProcessor for SharedPluginProcessor {
    fn process(&mut self, samples: &mut [f32], channels: u16, sample_rate: u32) {
        if let Ok(mut plugin) = self.inner.lock() {
            plugin.process(samples, channels, sample_rate);
        }
    }
    fn reset(&mut self) {
        if let Ok(mut plugin) = self.inner.lock() {
            plugin.reset();
        }
    }
}
