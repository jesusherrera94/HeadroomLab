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
    ClipboardPort, DoomPort, FileWatcherPort, ProjectFileSystemPort, ProjectGeneratorPort,
    TerminalPort,
};
use crate::application::recent_projects_service::RecentProjectsService;
use crate::application::simulator_service::SimulatorService;
use crate::domain::project::{RecentProject, sanitize_target};
use crate::domain::terminal::BuildKind;
use crate::presentation::components::organisms::code_pane;
use crate::presentation::editor_controller::{self, EditorState};
use crate::presentation::initial_controller::{self, InitialState, ProjectIntent};
use crate::presentation::simulation_controller;
use crate::presentation::terminal_controller;
use crate::presentation::window_manager::WindowManager;
use crate::presentation::windows::{
    doom_window, editor_window, graph_window, initial_window, simulator_window, splash_window,
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
    terminal: Rc<dyn TerminalPort>,
    doom: Rc<dyn DoomPort>,
    windows: WindowManager,
    /// Build & Run is waiting on a build: the simulator's window exists but is
    /// kept hidden until the build says whether it earned the right to appear.
    /// See `prepare_simulator` for why the window is created this early.
    simulator_awaiting_build: bool,

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
        terminal: Rc<dyn TerminalPort>,
        doom: Rc<dyn DoomPort>,
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
            terminal,
            doom,
            windows: WindowManager::default(),
            simulator_awaiting_build: false,
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
            self.terminal.clone(),
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

        // The toolbar's build buttons run their `make` target in the terminal's
        // Build tab.
        if let Some(state) = self.editor.as_mut() {
            let kind = if requests.build_run {
                Some(BuildKind::Dylib)
            } else if requests.compile {
                Some(BuildKind::Firmware)
            } else {
                None
            };

            if let Some(kind) = kind {
                let build = terminal_controller::run_build(&mut state.terminal, kind);
                if let Some(error) = build.error {
                    state.explorer.error = Some(error);
                }
            }
        }

        // "Open emulator" shows the simulator on whatever library is on disk.
        if requests.open_emulator {
            self.launch_simulator();
        }
        // Clicking DOOM.666 opens the game. Created here, on the click's
        // frame, for the same reason `prepare_simulator` documents: a viewport
        // window can only be created while eframe's event-loop TLS is set.
        if requests.open_doom {
            self.windows.open_doom(&self.doom);
        }
        // Build & Run creates the simulator's window now but leaves it hidden;
        // whether it is ever shown depends on the build.
        if requests.build_run {
            self.prepare_simulator();
        }
        // The build succeeded: reveal the window and load what it produced.
        if requests.reload_plugin {
            self.reveal_simulator();
        }
        // The build failed: take the hidden window away again. A library that
        // did not build must never be run.
        if requests.build_failed {
            self.discard_pending_simulator();
        }
        if close_requested || requests.quit_confirmed {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }

    /// Opens the simulator on the current project's built dynamic library
    /// (`<project>/build/lib<target>.<ext>`). A missing/unbuilt library surfaces
    /// through the simulator's existing error banner.
    fn launch_simulator(&mut self) {
        let path = self.effect_library_path();
        let state = self.windows.open_simulator();
        match self.sim_service.load_plugin(&path) {
            Ok(()) => self.graph_service.set_plugin_path(&path),
            Err(e) => {
                state.error_message = e.to_string();
                state.show_error = true;
            }
        }
    }

    /// Creates the simulator's window for a Build & Run, but **hidden**, and
    /// loads nothing into it yet.
    ///
    /// The split between creating and showing is what makes this safe *and*
    /// correct. Creating a viewport's window can only happen on a frame that has
    /// eframe's thread-local event loop set (`wgpu_integration.rs:1082`) — true
    /// of this click, false of the frame a finished build lands on, because this
    /// app hides its root window and eframe paints invisible windows straight
    /// from `new_events`, the one handler it never wraps (`run.rs:343` → `:222`).
    /// Creating it there fails silently and egui then asserts.
    ///
    /// Showing it, by contrast, is just a viewport command and needs nothing.
    /// So the window is made here and only revealed if the build earns it —
    /// which is why a failed build still never opens the emulator.
    fn prepare_simulator(&mut self) {
        if self.windows.simulator.is_some() {
            // Already on screen: nothing to hide, and nothing to create.
            return;
        }
        self.windows.open_simulator();
        self.simulator_awaiting_build = true;
    }

    /// The build succeeded: show the window prepared above and load the library
    /// it produced. Never creates a window.
    fn reveal_simulator(&mut self) {
        self.simulator_awaiting_build = false;
        let Some(state) = self.windows.simulator.as_mut() else {
            // The user closed it while the build ran — they have said they do
            // not want it, and opening one here is the unsafe case anyway.
            return;
        };
        state.focus_requested = true;

        let path = self.effect_library_path();
        match self.sim_service.load_plugin(&path) {
            Ok(()) => self.graph_service.set_plugin_path(&path),
            Err(e) => {
                if let Some(state) = self.windows.simulator.as_mut() {
                    state.error_message = e.to_string();
                    state.show_error = true;
                }
            }
        }
    }

    /// The build failed: drop the window that was waiting on it, so nothing is
    /// ever shown running a library that did not build.
    ///
    /// Only touches a simulator this build was holding hidden — one the user
    /// opened themselves stays open.
    fn discard_pending_simulator(&mut self) {
        if self.simulator_awaiting_build {
            self.simulator_awaiting_build = false;
            self.windows.close_simulator();
        }
    }

    /// `<project>/build/lib<target>.<ext>`, or empty when no project is open —
    /// a missing library surfaces through the simulator's error banner.
    fn effect_library_path(&self) -> String {
        self.current_project
            .as_ref()
            .and_then(effect_dylib_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
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
                .with_inner_size([640.0, 640.0])
                // Hidden while a Build & Run is in flight: the window has to
                // exist by now (it can only be created on a frame like the one
                // the button was clicked on) but must not be seen until the
                // build has succeeded.
                .with_visible(!self.simulator_awaiting_build),
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

    fn show_doom_viewport(&mut self, ctx: &egui::Context) {
        let Some(state) = self.windows.doom.as_mut() else {
            return;
        };

        let viewport_id = ViewportId::from_hash_of("doom_window");
        if state.focus_requested {
            state.focus_requested = false;
            ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
        }

        let mut close_requested = false;
        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab - DOOM.666")
                // 3× the engine's 320×200 frame, stretched to 4:3 the way the
                // original was.
                .with_inner_size([960.0, 720.0]),
            |ui, _class| {
                doom_window::show(ui, state);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            },
        );

        if close_requested {
            self.windows.close_doom();
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
                self.show_doom_viewport(&ctx);
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
