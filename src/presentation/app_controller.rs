//! The eframe application: composes the root window and drives the child
//! viewports (simulator + graph), routing view events into the controllers.

use std::rc::Rc;
use std::time::Duration;

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};
use rfd::FileDialog;

use crate::application::graph_service::GraphService;
use crate::application::simulator_service::SimulatorService;
use crate::presentation::window_manager::WindowManager;
use crate::presentation::simulation_controller;
use crate::presentation::windows::{app_window, graph_window, simulator_window};

/// Cadence of the playhead/graph sync, matching the old UI timers.
const TICK_INTERVAL: Duration = Duration::from_millis(100);

pub struct HeadroomApp {
    sim_service: Rc<SimulatorService>,
    graph_service: Rc<GraphService>,
    windows: WindowManager,
    effect_build_path: String,
}

impl HeadroomApp {
    pub fn new(sim_service: Rc<SimulatorService>, graph_service: Rc<GraphService>) -> Self {
        Self {
            sim_service,
            graph_service,
            windows: WindowManager::default(),
            effect_build_path: String::new(),
        }
    }

    fn browse_effect_build(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("Dynamic Library", &["dylib", "so", "dll"])
            .pick_file()
        {
            self.effect_build_path = path.display().to_string();
        }
    }

    fn launch_simulator(&mut self) {
        let path = self.effect_build_path.clone();
        let state = self.windows.open_simulator();
        match self.sim_service.load_plugin(&path) {
            Ok(()) => self.graph_service.set_plugin_path(&path),
            Err(e) => {
                state.error_message = e.to_string();
                state.show_error = true;
            }
        }
    }

    fn show_simulator_viewport(&mut self, ctx: &egui::Context) {
        let Some(state) = self.windows.simulator.as_mut() else { return };
        simulation_controller::tick(state, &self.sim_service);

        let viewport_id = ViewportId::from_hash_of("simulator_window");
        if state.focus_requested {
            state.focus_requested = false;
            ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
        }

        let sim_service = &self.sim_service;
        let graph_service = &self.graph_service;
        let mut open_graph = false;
        let mut close_requested = false;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab - Hardware Simulator")
                .with_inner_size([640.0, 640.0]),
            |ui, _class| {
                let events = simulator_window::show(ui, state);
                let requests =
                    simulation_controller::handle_events(state, events, sim_service, graph_service);
                open_graph = requests.open_graph;
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            },
        );

        if close_requested {
            self.windows.close_simulator();
        }
        if open_graph {
            self.windows.open_graph(&self.graph_service);
        }
    }

    fn show_graph_viewport(&mut self, ctx: &egui::Context) {
        let Some(session) = self.windows.graph.as_mut() else { return };
        session.tick(&self.graph_service);

        let viewport_id = ViewportId::from_hash_of("graph_window");
        if session.focus_requested {
            session.focus_requested = false;
            ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
        }

        let mut close_requested = false;
        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab - Signal Graph")
                .with_inner_size([1040.0, 760.0]),
            |ui, _class| {
                graph_window::show(ui, session);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            },
        );

        if close_requested {
            self.windows.close_graph();
        }
    }
}

impl eframe::App for HeadroomApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Replaces the old UI timers: keeps the playhead slider and the graph
        // worker polling alive even with no user input.
        ctx.request_repaint_after(TICK_INTERVAL);

        let events = app_window::show(ui, &mut self.effect_build_path);
        if events.browse_clicked {
            self.browse_effect_build();
        }
        if events.launch_clicked {
            self.launch_simulator();
        }

        self.show_simulator_viewport(&ctx);
        self.show_graph_viewport(&ctx);

        // Closing the main window quits the app; child viewports die with it.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.windows.close_all();
        }
    }
}