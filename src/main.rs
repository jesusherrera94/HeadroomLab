#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::error::Error;
use std::rc::Rc;

use HeadroomLab::{
    AppWindow,
    application::counter_service::CounterService,
    infrastructure::in_memory_counter_repo::InMemoryCounterRepo,
    presentation::{app_controller, window_manager::WindowManager},
};

use HeadroomLab::*; // trait ComponentHandle

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;
    // Compose dependencies (the only place that picks concrete impls)
    let repo = InMemoryCounterRepo::new(42);
    let service = Rc::new(CounterService::new(repo));
    let windows = Rc::new(WindowManager::default());
    app_controller::bind(&ui, service, windows);
    ui.run()?;
    Ok(())
}