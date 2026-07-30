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
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};

use crate::application::file_system_service::FileSystemService;
use crate::application::graph_service::GraphService;
use crate::application::ports::{
    ClipboardPort, FileWatcherPort, ProjectFileSystemPort, ProjectGeneratorPort,
};
use crate::application::recent_projects_service::RecentProjectsService;
use crate::application::simulator_service::SimulatorService;
use crate::domain::project::{RecentProject, sanitize_target};
use crate::presentation::components::organisms::code_pane;
use crate::presentation::editor_controller::{self, EditorState};
use crate::presentation::initial_controller::{self, InitialState, ProjectIntent};
use crate::presentation::simulation_controller;
use crate::presentation::window_manager::WindowManager;
use crate::presentation::windows::{
    editor_window, graph_window, initial_window, simulator_window, splash_window,
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
    file_system: Rc<dyn ProjectFileSystemPort>,
    fs_service: Rc<FileSystemService>,
    file_watcher: Rc<dyn FileWatcherPort>,
    clipboard: Rc<dyn ClipboardPort>,
    windows: WindowManager,

    screen: Screen,
    splash_started: Instant,
    initial: InitialState,
    current_project: Option<RecentProject>,
    editor: Option<EditorState>,
}

impl HeadroomApp {
    // The composition root's constructor: one parameter per port `main.rs`
    // chooses an implementation for. Grouping them into a struct would only move
    // the same list somewhere else.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sim_service: Rc<SimulatorService>,
        graph_service: Rc<GraphService>,
        recents: Rc<RefCell<RecentProjectsService>>,
        generator: Rc<dyn ProjectGeneratorPort>,
        file_system: Rc<dyn ProjectFileSystemPort>,
        fs_service: Rc<FileSystemService>,
        file_watcher: Rc<dyn FileWatcherPort>,
        clipboard: Rc<dyn ClipboardPort>,
    ) -> Self {
        Self {
            sim_service,
            graph_service,
            recents,
            generator,
            file_system,
            fs_service,
            file_watcher,
            clipboard,
            windows: WindowManager::default(),
            screen: Screen::Splash,
            splash_started: Instant::now(),
            initial: InitialState::default(),
            current_project: None,
            editor: None,
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

    /// Records the project in Recents and navigates to the Editor.
    fn open_project(&mut self, project: RecentProject) {
        self.recents.borrow_mut().record(project.clone());
        self.editor = Some(EditorState::new(
            &project,
            self.file_system.clone(),
            self.fs_service.clone(),
            self.file_watcher.clone(),
            self.clipboard.clone(),
        ));
        self.current_project = Some(project);
        self.screen = Screen::Editor;
        // The Initial viewport stops being shown next frame (window closes).
    }

    // -- Editor (IDE shell) ------------------------------------------------

    fn show_editor_viewport(&mut self, ctx: &egui::Context) {
        let Some(state) = self.editor.as_mut() else {
            return;
        };

        // Drain the filesystem watcher and refresh any changed directories.
        editor_controller::tick(state);

        let viewport_id = ViewportId::from_hash_of("editor_window");
        if state.focus_requested {
            state.focus_requested = false;
            ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
        }

        let mut requests = editor_controller::EditorRequests::default();
        let mut close_requested = false;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab - Editor")
                .with_inner_size([1000.0, 640.0]),
            |ui, _class| {
                let events = editor_window::show(ui, state);
                requests = editor_controller::handle_events(state, events);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());

                // Tabs that went away this frame — closed, or pruned by the
                // watcher — release their cursor, scroll and undo history here.
                // The controller stays egui-free; only the view touches memory.
                for tab in editor_controller::take_discarded_tabs(state) {
                    code_pane::forget_editor_state(ui.ctx(), tab);
                }

                // Closing the Editor quits the whole app, so unsaved buffers get
                // one chance to be rescued: cancel the OS close and raise the
                // confirmation instead. The user's answer arrives next frame as
                // `quit_confirmed`.
                if close_requested && editor_controller::request_quit(state) {
                    ui.ctx().send_viewport_cmd(ViewportCommand::CancelClose);
                    close_requested = false;
                }
            },
        );

        // "Open emulator" launches the simulator on the project's built dylib.
        // "Build & run" and "Compile" are wired to the terminal in a later task.
        if requests.open_emulator {
            self.launch_simulator();
        }
        if close_requested || requests.quit_confirmed {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }

    /// Opens the simulator on the current project's built dynamic library
    /// (`<project>/build/lib<target>.<ext>`). A missing/unbuilt library surfaces
    /// through the simulator's existing error banner.
    fn launch_simulator(&mut self) {
        let path = self
            .current_project
            .as_ref()
            .and_then(effect_dylib_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

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

/// Derives the path to a project's built simulator library, matching the
/// generated `Makefile` (`build/lib<target>.<ext>`, target = sanitized name,
/// ext = platform shared-library suffix). Returns `None` when the name yields
/// no valid target.
fn effect_dylib_path(project: &RecentProject) -> Option<PathBuf> {
    let target = sanitize_target(&project.name)?;
    let ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    Some(
        project
            .path
            .join("build")
            .join(format!("lib{target}.{ext}")),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dylib_path_sanitizes_name_and_uses_platform_ext() {
        let project = RecentProject::new("My Fuzz", "/tmp/projects/my-fuzz");
        let ext = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        assert_eq!(
            effect_dylib_path(&project),
            Some(PathBuf::from(format!(
                "/tmp/projects/my-fuzz/build/libmy_fuzz.{ext}"
            )))
        );
    }

    #[test]
    fn dylib_path_none_when_name_has_no_valid_target() {
        let project = RecentProject::new("###", "/tmp/x");
        assert_eq!(effect_dylib_path(&project), None);
    }
}
