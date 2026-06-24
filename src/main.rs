#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::error::Error;
use std::rc::Rc;

use HeadroomLab::{
    AppWindow,
    application::{counter_service::CounterService, simulator_service::SimulatorService},
    infrastructure::{in_memory_counter_repo::InMemoryCounterRepo, audio_engine::AudioEngine},
    presentation::{app_controller, window_manager::WindowManager, simulation_controller},
};

use HeadroomLab::*; // trait ComponentHandle

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;
    // Compose dependencies (the only place that picks concrete impls)
    let repo = InMemoryCounterRepo::new(42);
    let service = Rc::new(CounterService::new(repo));
    
    let audio_engine = Rc::new(AudioEngine::new());
    let sim_service = Rc::new(SimulatorService::new(audio_engine));

    let windows = Rc::new(WindowManager::default());
    app_controller::bind(&ui, service, sim_service, windows);
    ui.run()?;
    Ok(())
}