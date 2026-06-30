/// Describes the physical controls a hardware platform exposes.
pub struct HardwareLayout {
    pub knob_count: usize,
    pub toggle_switch_count: usize,
    pub footswitch_count: usize,
}

/// Abstraction over a hardware platform (Hothouse, Arduino, Raspberry Pi…).
/// Implementations live in `infrastructure`.
pub trait HardwarePlatform {
    fn platform_name(&self) -> &str;
    fn layout(&self) -> HardwareLayout;
}
