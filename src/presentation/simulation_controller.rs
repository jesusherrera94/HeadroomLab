use std::rc::Rc;
use slint::ComponentHandle;
use rfd::FileDialog;

use crate::SimulatorWindow;
use crate::application::simulator_service::SimulatorService;
use crate::presentation::window_manager::WindowManager;

pub fn bind(
    ui: &SimulatorWindow,
    service: Rc<SimulatorService>,
    windows: Rc<WindowManager>,
) {
    let ui_handle = ui.as_weak();
    let service_clone = service.clone();

    ui.on_upload_audio(move || {
        let ui = ui_handle.unwrap();
        
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
    ui.on_open_graph(move || {
        windows_clone.open_graph_window();
    });
}
