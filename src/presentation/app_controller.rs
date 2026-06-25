// src/presentation/app_controller.rs
use std::rc::Rc;
use crate::AppWindow;
use slint::ComponentHandle; // used for as_weak
use crate::application::ports::CounterRepository;
use crate::application::counter_service::CounterService;
use crate::application::simulator_service::SimulatorService;
use crate::presentation::window_manager::{ WindowManager};
use crate::presentation::simulation_controller;

pub fn bind<R: CounterRepository + 'static>(
    ui: &AppWindow,
    service: Rc<CounterService<R>>,
    sim_service: Rc<SimulatorService>,
    windows: Rc<WindowManager>,
) {
    
    ui.on_request_increase_value({
        let ui_handle = ui.as_weak();
        let service = service.clone();
        move || {
            let new_value = service.increase();
            ui_handle.unwrap().set_counter(new_value);
        }
    });

    ui.on_open_new_window({
        let windows = windows.clone();
        move || windows.open_hello()
    });

    ui.on_open_simulator_window({
        let windows = windows.clone();
        let sim_service = sim_service.clone();
        move || {
            let sim_window = windows.open_simulator();
            simulation_controller::bind(&sim_window, sim_service.clone(), windows.clone());
        }
    });

    ui.window().on_close_requested({
        let windows = windows.clone();
        move || {
            windows.close_all();
            slint::quit_event_loop().unwrap();
            slint::CloseRequestResponse::HideWindow
        }
    });
}
