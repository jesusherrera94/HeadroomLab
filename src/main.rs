#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui::ViewportBuilder;

use HeadroomLab::{
    application::{
        graph_service::GraphService,
        ports::{AudioEnginePort, ProjectFileSystemPort, ProjectGeneratorPort},
        recent_projects_service::RecentProjectsService,
        simulator_service::SimulatorService,
    },
    infrastructure::{
        audio_engine::AudioEngine, project_generator::TemplateProjectGenerator,
        recent_projects_store::FileRecentProjectsStore,
        std_fs_project_file_system::StdFsProjectFileSystem,
    },
    presentation::{app_controller::HeadroomApp, theme},
};

fn main() -> eframe::Result<()> {
    // Compose dependencies (the only place that picks concrete impls)
    let audio_engine: Rc<dyn AudioEnginePort> = Rc::new(AudioEngine::new());
    let sim_service = Rc::new(SimulatorService::new(audio_engine.clone()));
    let graph_service = Rc::new(GraphService::new(audio_engine.clone()));

    let recents_store = Rc::new(FileRecentProjectsStore::new());
    let recents = Rc::new(RefCell::new(RecentProjectsService::new(recents_store)));

    let generator: Rc<dyn ProjectGeneratorPort> = Rc::new(TemplateProjectGenerator::new());

    let file_system: Rc<dyn ProjectFileSystemPort> = Rc::new(StdFsProjectFileSystem::new());

    // The root window is the Splash screen.
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("HeadroomLab")
            .with_inner_size([380.0, 240.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "HeadroomLab",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            theme::install_icon_font(&cc.egui_ctx);
            Ok(Box::new(HeadroomApp::new(
                sim_service,
                graph_service,
                recents,
                generator,
                file_system,
            )))
        }),
    )
}
