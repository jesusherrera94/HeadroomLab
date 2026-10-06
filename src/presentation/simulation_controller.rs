use rfd::FileDialog;

use crate::application::graph_service::GraphService;
use crate::application::simulator_service::SimulatorService;
use crate::presentation::components::organisms::hardware_controls_panel::HardwareControlsState;
use crate::presentation::windows::simulator_window::SimulatorViewEvents;

pub struct SimulatorState {
    pub has_audio: bool,
    pub is_playing: bool,
    pub is_bypassed: bool,
    pub current_time: f32,
    pub duration: f32,
    pub error_message: String,
    pub show_error: bool,
    pub hardware: HardwareControlsState,
    pub focus_requested: bool,
}

impl Default for SimulatorState {
    fn default() -> Self {
        Self {
            has_audio: false,
            is_playing: false,
            is_bypassed: false,
            current_time: 0.0,
            duration: 0.0,
            error_message: String::new(),
            show_error: false,
            hardware: HardwareControlsState::default(),
            focus_requested: false,
        }
    }
}

#[derive(Default)]
pub struct SimulatorRequests {
    pub open_graph: bool,
}

pub fn handle_events(
    state: &mut SimulatorState,
    events: SimulatorViewEvents,
    service: &SimulatorService,
    graph_service: &GraphService,
) -> SimulatorRequests {
    let mut requests = SimulatorRequests::default();

    if events.transport.upload_clicked {
        upload_audio(state, service, graph_service);
    }

    if events.transport.play_toggled {
        state.is_playing = !state.is_playing;
        if state.is_playing {
            service.play();
        } else {
            service.stop();
        }
    }

    if events.transport.bypass_toggled {
        state.is_bypassed = !state.is_bypassed;
        service.toggle_bypass(state.is_bypassed);
    }

    if let Some(time) = events.transport.seek_to {
        service.seek_to(time);
    }

    if events.transport.view_graph_clicked {
        requests.open_graph = true;
    }

    for (index, value) in events.hardware.knob_changes {
        service.set_knob(index, value);
        graph_service.set_knob(index, value);
    }
    for (index, position) in events.hardware.switch_changes {
        service.set_switch(index, position);
        graph_service.set_switch(index, position);
    }
    for (index, pressed) in events.hardware.footswitch_changes {
        service.set_footswitch(index, pressed);
        graph_service.set_footswitch(index, pressed);
    }

    requests
}

pub fn tick(state: &mut SimulatorState, service: &SimulatorService) {
    state.current_time = service.current_position();
    if !service.is_playing() && state.is_playing {
        state.is_playing = false;
        state.current_time = 0.0;
    }
}

fn upload_audio(
    state: &mut SimulatorState,
    service: &SimulatorService,
    graph_service: &GraphService,
) {
    println!("[SimulationController] Uploading audio file...");
    let Some(path) = FileDialog::new()
        .add_filter("Audio", &["wav", "mp3", "ogg"])
        .pick_file()
    else {
        return;
    };

    match service.handle_file_upload(path.display().to_string()) {
        Ok(duration) => {
            state.has_audio = true;
            state.duration = duration;
            state.current_time = 0.0;
            state.is_playing = false;
            service.stop();
            graph_service.mark_dirty();
        }
        Err(e) => {
            println!("Validation failed: {:?}", e);
            state.error_message = format!("Audio file validation failed: {:?}", e);
            state.show_error = true;
        }
    }
}
