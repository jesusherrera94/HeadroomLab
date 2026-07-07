#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::rc::Rc;

use eframe::egui::ViewportBuilder;

use HeadroomLab::{
    application::{
        graph_service::GraphService, ports::AudioEnginePort, simulator_service::SimulatorService,
    },
    infrastructure::audio_engine::AudioEngine,
    presentation::{app_controller::HeadroomApp, theme},
};

fn main() -> eframe::Result<()> {
    // Compose dependencies (the only place that picks concrete impls)
    let audio_engine: Rc<dyn AudioEnginePort> = Rc::new(AudioEngine::new());
    let sim_service = Rc::new(SimulatorService::new(audio_engine.clone()));
    let graph_service = Rc::new(GraphService::new(audio_engine.clone()));

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("HeadroomLab")
            .with_inner_size([420.0, 130.0]),
        ..Default::default()
    };

    eframe::run_native(
        "HeadroomLab",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(HeadroomApp::new(sim_service, graph_service)))
        }),
    )
}
