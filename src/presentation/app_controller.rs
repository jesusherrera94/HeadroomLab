//! The eframe application and its screen state machine.
//!
//! The **root window is the Splash** at startup. After ~1s it hides the root
//! and shows the **Initial** window as its own child viewport; choosing a
//! project hides Initial and shows the **Editor** viewport (the reused
//! `app_window` placeholder), from which the Simulator and Graph child
//! viewports open exactly as before.
//!
//! Hiding the root is safe here: eframe keeps calling this `ui` as long as any
//! descendant viewport is visible, so the child viewports keep rendering.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};
use rfd::FileDialog;

use crate::application::graph_service::GraphService;
use crate::application::ports::ProjectGeneratorPort;
use crate::application::recent_projects_service::RecentProjectsService;
use crate::application::simulator_service::SimulatorService;
use crate::domain::project::RecentProject;
use crate::presentation::initial_controller::{self, InitialState, ProjectIntent};
use crate::presentation::simulation_controller;
use crate::presentation::window_manager::WindowManager;
use crate::presentation::windows::{
    app_window, graph_window, initial_window, simulator_window, splash_window,
};

/// Cadence of the playhead/graph sync, matching the old UI timers.
const TICK_INTERVAL: Duration = Duration::from_millis(100);
/// How long the mocked splash shows before auto-advancing to Initial.
const SPLASH_DURATION: Duration = Duration::from_millis(1000);

/// Which top-level screen is active. One OS window at a time.
enum Screen {
    Splash,
    Initial,
    Editor,
}

pub struct HeadroomApp {
    sim_service: Rc<SimulatorService>,
    graph_service: Rc<GraphService>,
    recents: Rc<RefCell<RecentProjectsService>>,
    generator: Rc<dyn ProjectGeneratorPort>,
    windows: WindowManager,
    effect_build_path: String,

    screen: Screen,
    splash_started: Instant,
    initial: InitialState,
    #[allow(dead_code)] // carried for the real Editor; placeholder ignores it for now.
    current_project: Option<RecentProject>,
}

impl HeadroomApp {
    pub fn new(
        sim_service: Rc<SimulatorService>,
        graph_service: Rc<GraphService>,
        recents: Rc<RefCell<RecentProjectsService>>,
        generator: Rc<dyn ProjectGeneratorPort>,
    ) -> Self {
        Self {
            sim_service,
            graph_service,
            recents,
            generator,
            windows: WindowManager::default(),
            effect_build_path: String::new(),
            screen: Screen::Splash,
            splash_started: Instant::now(),
            initial: InitialState::default(),
            current_project: None,
        }
    }

    // -- Splash ------------------------------------------------------------

    fn show_splash(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        splash_window::show(ui);
        if self.splash_started.elapsed() >= SPLASH_DURATION {
            self.screen = Screen::Initial;
            self.initial.focus_requested = true;
            // Hide the root; the Initial child viewport keeps this `ui` alive.
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(false));
        }
    }

    // -- Initial -----------------------------------------------------------

    fn show_initial_viewport(&mut self, ctx: &egui::Context) {
        let viewport_id = ViewportId::from_hash_of("initial_window");
        if self.initial.focus_requested {
            self.initial.focus_requested = false;
            ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
        }

        // Snapshot the (≤5) recents so the render doesn't hold a borrow across
        // the event handling that mutates them.
        let recents = self.recents.borrow().list().to_vec();
        let state = &mut self.initial;
        let mut events = None;
        let mut close_requested = false;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab")
                .with_inner_size([460.0, 420.0])
                .with_resizable(false),
            |ui, _class| {
                events = Some(initial_window::show(ui, state, &recents));
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            },
        );

        if let Some(events) = events
            && let Some(intent) = initial_controller::handle_events(state, events, &recents)
        {
            self.handle_intent(intent);
        }
        if close_requested {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }

    /// Acts on a chosen project. `Create` generates the files first and only
    /// records/navigates on success (staying on Initial with an inline error
    /// otherwise, D10); `Open`/Recent record and navigate directly.
    fn handle_intent(&mut self, intent: ProjectIntent) {
        match intent {
            ProjectIntent::Open(project) => self.open_project(project),
            ProjectIntent::Create(project) => {
                match self.generator.generate(&project.name, &project.path) {
                    Ok(()) => {
                        self.initial.close_modal();
                        self.open_project(project);
                    }
                    Err(e) => self.initial.generation_error = Some(e.to_string()),
                }
            }
        }
    }

    /// Records the project in Recents and navigates to the Editor placeholder.
    fn open_project(&mut self, project: RecentProject) {
        self.recents.borrow_mut().record(project.clone());
        self.current_project = Some(project);
        self.screen = Screen::Editor;
        // The Initial viewport stops being shown next frame (window closes).
    }

    // -- Editor (placeholder = the reused app_window) ----------------------

    fn show_editor_viewport(&mut self, ctx: &egui::Context) {
        let viewport_id = ViewportId::from_hash_of("editor_window");
        let effect_build_path = &mut self.effect_build_path;
        let mut events = app_window::AppWindowEvents::default();
        let mut close_requested = false;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab - Editor")
                .with_inner_size([1000.0, 640.0]),
            |ui, _class| {
                events = app_window::show(ui, effect_build_path);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            },
        );

        if events.browse_clicked {
            self.browse_effect_build();
        }
        if events.launch_clicked {
            self.launch_simulator();
        }
        if close_requested {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
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
        let Some(state) = self.windows.simulator.as_mut() else {
            return;
        };
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
        let Some(session) = self.windows.graph.as_mut() else {
            return;
        };
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
        // Keeps the splash timer, playhead and graph worker ticking with no
        // user input.
        ctx.request_repaint_after(TICK_INTERVAL);

        match self.screen {
            Screen::Splash => self.show_splash(ui, &ctx),
            Screen::Initial => self.show_initial_viewport(&ctx),
            Screen::Editor => {
                self.show_editor_viewport(&ctx);
                self.show_simulator_viewport(&ctx);
                self.show_graph_viewport(&ctx);
            }
        }

        // Closing the (hidden) root quits the app; child viewports die with it.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.windows.close_all();
        }
    }
}
