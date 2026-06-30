use std::rc::Rc;
use std::time::Duration;
use slint::{ComponentHandle, Timer, TimerMode};
use rfd::FileDialog;

use crate::SimulatorWindow;
use crate::application::simulator_service::SimulatorService;
use crate::application::graph_service::GraphService;
use crate::presentation::window_manager::WindowManager;
use crate::presentation::graph_controller;

pub fn bind(
    ui: &SimulatorWindow,
    service: Rc<SimulatorService>,
    graph_service: Rc<GraphService>,
    windows: Rc<WindowManager>,
) -> Timer {
    let ui_handle = ui.as_weak();
    let service_clone = service.clone();
    let graph_service_clone = graph_service.clone();

    ui.on_upload_audio(move || {
        let ui = ui_handle.unwrap();
        println!("[SimulationController] Uploading audio file...");
        // Open native file dialog
        if let Some(path) = FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "ogg"])
            .pick_file() 
        {
            match service_clone.handle_file_upload(path.display().to_string()) {
                Ok(duration) => {
                    ui.set_has_audio(true);
                    ui.set_duration(duration);
                    ui.set_current_time(0.0);
                    ui.set_is_playing(false);
                    service_clone.stop(); // Reset engine state
                    graph_service_clone.mark_dirty(); // Refresh an already-open graph window
                }
                Err(e) => {
                    println!("Validation failed: {:?}", e);
                    // Here you could trigger a Slint modal for error notification
                }
            }
        }
    });

    let ui_handle = ui.as_weak();
    let service_clone = service.clone();
    ui.on_toggle_play(move || {
        let ui = ui_handle.unwrap();
        let playing = !ui.get_is_playing();
        ui.set_is_playing(playing);
        if playing { service_clone.play(); } else { service_clone.stop(); }
    });

    let ui_handle = ui.as_weak();
    let service_clone = service.clone();
    ui.on_toggle_bypass(move || {
        let ui = ui_handle.unwrap();
        let bypassed = !ui.get_is_bypassed();
        ui.set_is_bypassed(bypassed);
        service_clone.toggle_bypass(bypassed);
    });

    let service_clone = service.clone();
    ui.on_seek_audio(move |val| {
        service_clone.seek_to(val);
    });

    let windows_clone = windows.clone();
    let graph_service_clone = graph_service.clone();
    ui.on_open_graph(move || {
        let graph_window = windows_clone.open_graph();
        let timer = graph_controller::bind(&graph_window, graph_service_clone.clone());
        windows_clone.set_graph_timer(timer);
    });

    let service_clone = service.clone();
    let graph_service_clone = graph_service.clone();
    ui.on_knob_changed(move |index, value| {
        service_clone.set_knob(index as usize, value);
        graph_service_clone.set_knob(index as usize, value);
    });

    let service_clone = service.clone();
    let graph_service_clone = graph_service.clone();
    ui.on_switch_changed(move |index, position| {
        service_clone.set_switch(index as usize, position);
        graph_service_clone.set_switch(index as usize, position);
    });

    let service_clone = service.clone();
    let graph_service_clone = graph_service.clone();
    ui.on_footswitch_changed(move |index, pressed| {
        service_clone.set_footswitch(index as usize, pressed);
        graph_service_clone.set_footswitch(index as usize, pressed);
    });

    ui.on_error_dismissed(|| {});

    let ui_handle = ui.as_weak();
    let service_clone = service.clone();
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            let ui = match ui_handle.upgrade() {
                Some(u) => u,
                None => return,   // window was closed
            };
            // Sync slider position from the actual playhead
            ui.set_current_time(service_clone.current_position());
            // Detect natural end-of-track: engine stopped but UI still shows "Stop"
            if !service_clone.is_playing() && ui.get_is_playing() {
                ui.set_is_playing(false);
                ui.set_current_time(0.0);
            }
        },
    );
    timer
}
