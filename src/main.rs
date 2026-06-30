#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::error::Error;
use std::rc::Rc;

use HeadroomLab::{
    AppWindow,
    application::{counter_service::CounterService, simulator_service::SimulatorService, graph_service::GraphService, ports::AudioEnginePort},
    infrastructure::{in_memory_counter_repo::InMemoryCounterRepo, audio_engine::AudioEngine},
    presentation::{app_controller, window_manager::WindowManager},
};

use HeadroomLab::*; // trait ComponentHandle

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;
    // Compose dependencies (the only place that picks concrete impls)
    let repo = InMemoryCounterRepo::new(42);
    let service = Rc::new(CounterService::new(repo));

    let audio_engine: Rc<dyn AudioEnginePort> = Rc::new(AudioEngine::new());
    let sim_service = Rc::new(SimulatorService::new(audio_engine.clone()));
    let graph_service = Rc::new(GraphService::new(audio_engine.clone()));

    let windows = Rc::new(WindowManager::default());
    app_controller::bind(&ui, service, sim_service, graph_service, windows);

    ui.run()?;
    Ok(())
}