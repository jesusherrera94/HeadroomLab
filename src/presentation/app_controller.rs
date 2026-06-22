// src/presentation/app_controller.rs
use std::rc::Rc;
use crate::AppWindow;
use slint::ComponentHandle; // used for as_weak
use crate::application::ports::CounterRepository;
use crate::application::counter_service::CounterService;
use crate::presentation::window_manager::WindowManager;

pub fn bind<R: CounterRepository + 'static>(
    ui: &AppWindow,
    service: Rc<CounterService<R>>,
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

    ui.window().on_close_requested({
        let windows = windows.clone();
        move || {
            windows.close_all();
            slint::quit_event_loop().unwrap();
            slint::CloseRequestResponse::HideWindow
        }
    });
}
