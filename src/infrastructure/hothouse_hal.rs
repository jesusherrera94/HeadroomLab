use crate::domain::hardware_platform::{HardwareLayout, HardwarePlatform};

pub struct HothouseHardware;

impl HardwarePlatform for HothouseHardware {
    fn platform_name(&self) -> &str {
        "Cleveland Music Co. — Hothouse"
    }

    fn layout(&self) -> HardwareLayout {
        HardwareLayout {
            knob_count: 6,
            toggle_switch_count: 3,
            footswitch_count: 2,
        }
    }
}
