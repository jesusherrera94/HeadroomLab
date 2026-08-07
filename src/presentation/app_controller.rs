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
use crate::domain::menu::{
    self, MenuCommand, MenuContext, MenuModel, MenuSurface, TransportCommand, WindowId,
};
use crate::domain::project::{RecentProject, sanitize_target};
use crate::domain::terminal::BuildKind;
use crate::domain::text_document::line_comment;
use crate::presentation::components::organisms::{code_pane, menu_bar};
use crate::presentation::editor_controller::{self, EditorState};
use crate::presentation::initial_controller::{self, InitialState, ProjectIntent};
use crate::presentation::menu_controller::{self, OpenInitial};
use crate::presentation::simulation_controller;
use crate::presentation::terminal_controller;
use crate::presentation::theme;
use crate::presentation::window_manager::WindowManager;
use crate::presentation::windows::simulator_window::SimulatorViewEvents;
use crate::presentation::windows::{
    doom_window, editor_window, graph_window, initial_window, simulator_window, splash_window,
};

/// Cadence of the playhead/graph sync, matching the old UI timers.
const TICK_INTERVAL: Duration = Duration::from_millis(100);
/// How long the mocked splash shows before auto-advancing to Initial.
const SPLASH_DURATION: Duration = Duration::from_millis(1000);
/// How often the system clipboard is sampled for the Edit menu's Paste item.
/// Reading it every frame would be a round-trip to the owning process a hundred
/// times a second; on X11 that is a real cost for a menu row nobody is looking
/// at.
const CLIPBOARD_POLL: Duration = Duration::from_millis(500);
/// Where Help ▸ HeadroomLab Help goes (D11).
const HELP_URL: &str = "https://github.com/jesusherrera94/HeadroomLab#readme";

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

    /// The Initial window shown *beside* the Editor, so a project can be swapped
    /// without losing the one already open if the user backs out (D3).
    initial_open: bool,
    /// Which window the OS says has focus, sampled once per frame so the menu,
    /// its dispatch and the close handling all agree.
    focused: WindowId,
    /// A quit cleared the unsaved-work guard: close the app at the end of the
    /// frame, where the root viewport can be told to go.
    quit_requested: bool,
    /// The About box (S5).
    about_open: bool,
    /// The macOS menu bar. `None` off macOS, where each window draws its own.
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::presentation::native_menu::NativeMenu>,
    /// Whether the code area held keyboard focus at the end of the last frame.
    /// Sampled inside the Editor's viewport, since focus is per-viewport.
    code_area_focused: bool,
    /// Last sampled clipboard state, for the Edit menu's Paste item, and when it
    /// was taken.
    clipboard_ready: bool,
    clipboard_checked: Instant,
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
            initial_open: false,
            focused: WindowId::Splash,
            quit_requested: false,
            about_open: false,
            #[cfg(target_os = "macos")]
            native_menu: None,
            code_area_focused: false,
            clipboard_ready: false,
            clipboard_checked: Instant::now() - CLIPBOARD_POLL,
        }
    }

    /// The viewport the repaint tick is scheduled on: one the user can see, so
    /// the tick arrives as a wrapped winit event. See `ui` for why that matters.
    ///
    /// Prefers the screen's main window, falling back to any other open one if
    /// that has been minimised — a minimised window is painted through the same
    /// unwrapped path as a hidden one.
    fn ticking_viewport(&self, ctx: &egui::Context) -> ViewportId {
        let preferred = match self.screen {
            // The root *is* the splash, and it is still visible at this point.
            Screen::Splash => return ViewportId::ROOT,
            Screen::Initial => WindowId::Initial,
            Screen::Editor => WindowId::Editor,
        };

        let on_screen = |window: WindowId| {
            !ctx.input_for(Self::viewport_id(window), |input| {
                input.viewport().minimized.unwrap_or(false)
            })
        };

        if on_screen(preferred) {
            return Self::viewport_id(preferred);
        }
        // Minimised. Any other open window will keep the loop fed.
        let fallback = [
            (WindowId::Simulator, self.windows.simulator.is_some()),
            (WindowId::Graph, self.windows.graph.is_some()),
            (WindowId::Doom, self.windows.doom.is_some()),
            (WindowId::Initial, self.initial_open),
        ]
        .into_iter()
        .find(|(window, open)| *open && on_screen(*window))
        .map(|(window, _)| window);

        // With everything minimised the loop falls back to the unwrapped path.
        // That still runs — it just cannot create a window, which is why
        // `focus_or_open` un-minimises rather than opening in that state.
        Self::viewport_id(fallback.unwrap_or(preferred))
    }

    // -- Menus -------------------------------------------------------------

    /// The viewport id a window is rendered into. One place, so the focus probe
    /// and the focus *command* can never disagree about which window is which.
    fn viewport_id(window: WindowId) -> ViewportId {
        match window {
            WindowId::Splash => ViewportId::ROOT,
            WindowId::Initial => ViewportId::from_hash_of("initial_window"),
            WindowId::Editor => ViewportId::from_hash_of("editor_window"),
            WindowId::Simulator => ViewportId::from_hash_of("simulator_window"),
            WindowId::Graph => ViewportId::from_hash_of("graph_window"),
            WindowId::Doom => ViewportId::from_hash_of("doom_window"),
        }
    }

    /// Which window the OS says has focus.
    ///
    /// Falls back to whichever screen is showing when nothing is focused — the
    /// app has just been backgrounded, and greying the whole bar out because the
    /// user clicked another app would be wrong.
    fn focused_window(&self, ctx: &egui::Context) -> WindowId {
        let candidates: &[WindowId] = match self.screen {
            Screen::Splash => &[WindowId::Splash],
            Screen::Initial => &[WindowId::Initial],
            Screen::Editor => &[
                WindowId::Doom,
                WindowId::Graph,
                WindowId::Simulator,
                WindowId::Initial,
                WindowId::Editor,
            ],
        };

        let focused = candidates.iter().copied().find(|window| {
            ctx.input_for(Self::viewport_id(*window), |input| {
                input.viewport().focused.unwrap_or(false)
            })
        });

        focused.unwrap_or(match self.screen {
            Screen::Splash => WindowId::Splash,
            Screen::Initial => WindowId::Initial,
            Screen::Editor => WindowId::Editor,
        })
    }

    /// Assembles everything the enablement rules need for this frame.
    fn menu_context(&mut self, ctx: &egui::Context) -> MenuContext {
        // Sampled rather than read every frame — see `CLIPBOARD_POLL`.
        if self.clipboard_checked.elapsed() >= CLIPBOARD_POLL {
            self.clipboard_ready = self.clipboard.read().is_some();
            self.clipboard_checked = Instant::now();
        }

        let focused = self.focused_window(ctx);
        let simulator = self.windows.simulator.as_ref();

        let mut context = MenuContext {
            focused,
            has_project: self.editor.is_some(),
            recents: self
                .recents
                .borrow()
                .list()
                .iter()
                .map(|project| project.name.clone())
                .collect(),
            can_paste: self.clipboard_ready,
            simulator_open: simulator.is_some(),
            graph_open: self.windows.graph.is_some(),
            doom_open: self.windows.doom.is_some(),
            has_audio: simulator.is_some_and(|s| s.has_audio),
            is_playing: simulator.is_some_and(|s| s.is_playing),
            is_bypassed: simulator.is_some_and(|s| s.is_bypassed),
            ..MenuContext::default()
        };

        if let Some(editor) = self.editor.as_ref() {
            context.any_tab_dirty = editor_controller::has_unsaved_work(editor);
            context.build_running = matches!(
                editor.terminal.build_status(),
                Some((_, crate::domain::terminal::BuildStatus::Running))
            );

            if let Some(tab) = editor.tabs.get(editor.active_tab) {
                context.has_open_tab = true;
                context.active_tab_editable = tab.content.is_editable();
                context.active_tab_dirty = tab.unsaved();
                context.can_comment = line_comment(tab.language).is_some();
                // The selection lives in egui's memory for that buffer's
                // `TextEdit`, which is the only place it exists.
                context.has_selection =
                    egui::text_edit::TextEditState::load(ctx, code_pane::editor_id(tab.id))
                        .and_then(|state| state.cursor.char_range())
                        .is_some_and(|range| range.primary.index != range.secondary.index);
            }
        }

        context
    }

    /// Keeps the native bar in step and drains its clicks.
    ///
    /// The clicks are *returned*, never acted on here: several commands open a
    /// window, and that is only legal at the point in the frame the app
    /// controller chooses (see `prepare_simulator`).
    #[cfg(target_os = "macos")]
    fn sync_native_menu(&mut self, context: &MenuContext) -> Vec<MenuCommand> {
        use crate::presentation::native_menu::NativeMenu;

        let menu = self
            .native_menu
            .get_or_insert_with(|| NativeMenu::install(context));
        menu.sync(context);
        menu.poll()
    }

    #[cfg(not(target_os = "macos"))]
    fn sync_native_menu(&mut self, _context: &MenuContext) -> Vec<MenuCommand> {
        Vec::new()
    }

    /// Draws the in-window menu bar on the platforms that have no native one,
    /// returning whatever was clicked. A no-op on macOS, which has the real bar.
    fn window_menu_bar(
        ui: &mut egui::Ui,
        context: &MenuContext,
        window: WindowId,
    ) -> Option<MenuCommand> {
        if cfg!(target_os = "macos") {
            return None;
        }
        let model = MenuModel::build(context, MenuSurface::Window(window));
        if model.menus.is_empty() {
            return None;
        }

        let mut chosen = None;
        egui::Panel::top("menu_bar").show(ui, |ui| {
            chosen = menu_bar::menu_bar(ui, &model);
        });
        chosen
    }

    /// Runs one menu command. Every window-level effect lands here, at a point in
    /// the frame where opening a viewport is safe.
    fn run_menu_command(&mut self, command: MenuCommand) {
        let requests = menu_controller::dispatch(command, self.editor.as_mut());

        if let Some(mode) = requests.open_initial {
            self.open_initial_window(mode);
        }
        if let Some(index) = requests.open_recent {
            let project = self.recents.borrow().list().get(index).cloned();
            if let Some(project) = project {
                self.request_project(project);
            }
        }
        if requests.clear_recents {
            self.recents.borrow_mut().clear();
        }
        // About is app-level but has to be painted *somewhere*, so it lives on
        // whichever of the two primary windows exists — and that window is
        // raised, or the dialog would open behind the Simulator it was asked for
        // from.
        if requests.show_about {
            self.about_open = true;
            match self.editor.as_mut() {
                Some(editor) => editor.focus_requested = true,
                None => self.initial.focus_requested = true,
            }
        }
        if requests.open_help
            && let Err(e) = opener::open_browser(HELP_URL)
            && let Some(editor) = self.editor.as_mut()
        {
            editor.explorer.error = Some(format!("Could not open the documentation: {e}"));
        }
        if let Some(window) = requests.focus_window {
            self.focus_or_open(window);
        }
        if requests.open_emulator {
            self.launch_simulator();
        }
        if requests.build_run {
            self.run_build(BuildKind::Dylib);
            self.prepare_simulator();
        }
        if requests.compile {
            self.run_build(BuildKind::Firmware);
        }
        if let Some(action) = requests.transport {
            self.run_transport(action);
        }
        // Quitting and closing a window are the same guarded route the window's
        // own close button takes.
        if requests.quit {
            self.request_quit();
        }
        if requests.close_window {
            self.close_focused_window();
        }
    }

    /// Focuses `window`, opening it first when it is closed (D9).
    fn focus_or_open(&mut self, window: WindowId) {
        match window {
            WindowId::Editor => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.focus_requested = true;
                }
            }
            WindowId::Simulator => match self.windows.simulator.as_mut() {
                Some(state) => state.focus_requested = true,
                None => self.launch_simulator(),
            },
            WindowId::Graph => self.windows.open_graph(&self.graph_service),
            WindowId::Doom => self.windows.open_doom(&self.doom),
            WindowId::Initial => self.initial.focus_requested = true,
            WindowId::Splash => {}
        }
    }

    /// Closes whichever window has focus. The Editor is the app, so closing it
    /// is a quit and takes the unsaved-work route.
    fn close_focused_window(&mut self) {
        match self.focused {
            WindowId::Simulator => self.windows.close_simulator(),
            WindowId::Graph => self.windows.close_graph(),
            WindowId::Doom => self.windows.close_doom(),
            WindowId::Initial if self.initial_open => self.initial_open = false,
            _ => self.request_quit(),
        }
    }

    /// Raises the unsaved-work guard, and quits straight away when there is
    /// nothing to guard.
    fn request_quit(&mut self) {
        let guarded = self
            .editor
            .as_mut()
            .is_some_and(editor_controller::request_quit);
        if !guarded {
            self.quit_requested = true;
        }
    }

    fn run_build(&mut self, kind: BuildKind) {
        if let Some(state) = self.editor.as_mut() {
            let build = terminal_controller::run_build(&mut state.terminal, kind);
            if let Some(error) = build.error {
                state.explorer.error = Some(error);
            }
        }
    }

    /// Applies a Transport menu action to the Simulator, through the very same
    /// events its transport bar raises.
    fn run_transport(&mut self, action: TransportCommand) {
        let Some(state) = self.windows.simulator.as_mut() else {
            return;
        };
        let mut events = SimulatorViewEvents::default();
        match action {
            TransportCommand::LoadAudio => events.transport.upload_clicked = true,
            TransportCommand::PlayPause => events.transport.play_toggled = true,
            TransportCommand::Bypass => events.transport.bypass_toggled = true,
            TransportCommand::ViewGraph => events.transport.view_graph_clicked = true,
        }
        let requests = simulation_controller::handle_events(
            state,
            events,
            &self.sim_service,
            &self.graph_service,
        );
        if requests.open_graph {
            self.windows.open_graph(&self.graph_service);
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
        let viewport_id = Self::viewport_id(WindowId::Initial);
        if self.initial.focus_requested {
            self.initial.focus_requested = false;
            raise(ctx, viewport_id);
        }

        // Snapshot the (≤5) recents so the render doesn't hold a borrow across
        // the event handling that mutates them.
        let recents = self.recents.borrow().list().to_vec();
        // Only when there is no Editor: with one open, that window hosts About.
        let about_here = self.about_open && self.editor.is_none();
        let state = &mut self.initial;
        let mut events = None;
        let mut close_requested = false;
        let mut about_dismissed = false;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title("HeadroomLab")
                .with_inner_size([460.0, 420.0])
                .with_resizable(false),
            |ui, _class| {
                events = Some(initial_window::show(ui, state, &recents));
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
                if about_here {
                    about_dismissed = about_dialog(ui.ctx());
                }
            },
        );

        if about_dismissed {
            self.about_open = false;
        }

        if let Some(events) = events
            && let Some(intent) = initial_controller::handle_events(state, events, &recents)
        {
            self.handle_intent(intent);
        }
        if close_requested {
            // Beside the Editor this window is a project picker the user can
            // simply dismiss; on its own it is the app, and closing it quits.
            if self.initial_open {
                self.initial_open = false;
                self.initial.close_modal();
            } else {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            }
        }
    }

    /// Shows the Initial window beside whatever is already open, so File ▸ New /
    /// Open Project can reach it without discarding the current project (D3).
    fn open_initial_window(&mut self, mode: OpenInitial) {
        self.initial_open = true;
        self.initial.focus_requested = true;
        match mode {
            OpenInitial::CreateModal => self.initial.open_create_modal(),
            OpenInitial::Picker => self.initial.close_modal(),
        }
    }

    /// Acts on a chosen project. `Create` generates the files first and only
    /// records/navigates on success (staying on Initial with an inline error
    /// otherwise, D10); `Open`/Recent record and navigate directly.
    fn handle_intent(&mut self, intent: ProjectIntent) {
        match intent {
            ProjectIntent::Open(project) => self.request_project(project),
            ProjectIntent::Create(project) => {
                match self.generator.generate(&project.name, &project.path) {
                    Ok(()) => {
                        self.initial.close_modal();
                        self.request_project(project);
                    }
                    Err(e) => self.initial.generation_error = Some(e.to_string()),
                }
            }
        }
    }

    /// Moves to `project`, guarding unsaved work in whatever is open first.
    ///
    /// With no Editor open this is the plain first-launch path. With one open it
    /// is a *switch*: the confirmation carries the destination, and the answer
    /// arrives on a later frame as `EditorRequests::switch_project`.
    fn request_project(&mut self, project: RecentProject) {
        let guarded = self.editor.as_mut().is_some_and(|editor| {
            editor_controller::request_switch_project(editor, project.clone())
        });
        if guarded {
            // The Editor owns the modal, so it has to be the window in front.
            if let Some(editor) = self.editor.as_mut() {
                editor.focus_requested = true;
            }
            return;
        }
        self.open_project(project);
    }

    /// Records the project in Recents and rebuilds the Editor on it.
    ///
    /// Also the switch path: `EditorState::new` starts a fresh tree, watcher and
    /// terminal, and dropping the old state takes its PTYs with it. The child
    /// windows go too — they hold the *previous* project's dylib, and a Simulator
    /// left running one belonging to a project that is no longer open would be
    /// showing a lie.
    fn open_project(&mut self, project: RecentProject) {
        self.recents.borrow_mut().record(project.clone());
        self.windows.close_all();
        self.simulator_awaiting_build = false;
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
        self.initial_open = false;
        self.initial.close_modal();
        // The Initial viewport stops being shown next frame (window closes).
    }

    // -- Editor (IDE shell) ------------------------------------------------

    fn show_editor_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
        let Some(state) = self.editor.as_mut() else {
            return;
        };

        // Drain the filesystem watcher and refresh any changed directories.
        editor_controller::tick(state);

        let viewport_id = Self::viewport_id(WindowId::Editor);
        if state.focus_requested {
            state.focus_requested = false;
            raise(ctx, viewport_id);
        }

        let title = editor_title(state);
        let mut requests = editor_controller::EditorRequests::default();
        let mut close_requested = false;
        let mut menu_command = None;
        let about_open = self.about_open;
        let mut about_dismissed = false;
        let mut code_area_focused = false;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([1000.0, 640.0]),
            |ui, _class| {
                menu_command = Self::window_menu_bar(ui, context, WindowId::Editor);
                let events = editor_window::show(ui, state);
                requests = editor_controller::handle_events(state, events);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());

                if about_open {
                    about_dismissed = about_dialog(ui.ctx());
                }

                // Sampled here, inside the Editor's viewport, because focus is
                // per-viewport — see `menu_context`.
                code_area_focused = state
                    .tabs
                    .get(state.active_tab)
                    .map(|tab| code_pane::editor_id(tab.id))
                    .is_some_and(|id| ui.ctx().memory(|memory| memory.has_focus(id)));

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

        self.code_area_focused = code_area_focused;
        if about_dismissed {
            self.about_open = false;
        }

        // The toolbar's build buttons run their `make` target in the terminal's
        // Build tab.
        if requests.build_run {
            self.run_build(BuildKind::Dylib);
        } else if requests.compile {
            self.run_build(BuildKind::Firmware);
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
        // ⇧⌘W / ⌘Q take exactly the route the window's close button takes.
        if requests.quit_requested {
            self.request_quit();
        }
        // The unsaved-work modal was answered in favour of another project.
        if let Some(project) = requests.switch_project {
            self.open_project(project);
        }
        if close_requested || requests.quit_confirmed {
            self.quit_requested = true;
        }

        // Last, and outside the closure: several commands open a window, which
        // is only legal here — see `prepare_simulator`.
        if let Some(command) = menu_command {
            self.run_menu_command(command);
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

    fn show_simulator_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
        let Some(state) = self.windows.simulator.as_mut() else {
            return;
        };
        simulation_controller::tick(state, &self.sim_service);

        let viewport_id = Self::viewport_id(WindowId::Simulator);
        if state.focus_requested {
            state.focus_requested = false;
            raise(ctx, viewport_id);
        }

        let title = window_title(self.current_project.as_ref(), "Hardware Simulator");
        let sim_service = &self.sim_service;
        let graph_service = &self.graph_service;
        let mut open_graph = false;
        let mut close_requested = false;
        let mut menu_command = None;

        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([640.0, 640.0])
                // Hidden while a Build & Run is in flight: the window has to
                // exist by now (it can only be created on a frame like the one
                // the button was clicked on) but must not be seen until the
                // build has succeeded.
                .with_visible(!self.simulator_awaiting_build),
            |ui, _class| {
                menu_command = Self::window_menu_bar(ui, context, WindowId::Simulator);
                close_requested |= close_window_pressed(ui.ctx());
                let events = simulator_window::show(ui, state);
                let requests =
                    simulation_controller::handle_events(state, events, sim_service, graph_service);
                open_graph = requests.open_graph;
                close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            },
        );

        if close_requested {
            // Takes the Graph with it: the Graph renders the processed signal
            // for the plugin this window loaded, so on its own it would sit
            // there showing a signal nothing is producing any more.
            self.windows.close_simulator();
        }
        if open_graph {
            self.windows.open_graph(&self.graph_service);
        }
        if let Some(command) = menu_command {
            self.run_menu_command(command);
        }
    }

    fn show_doom_viewport(&mut self, ctx: &egui::Context) {
        let Some(state) = self.windows.doom.as_mut() else {
            return;
        };

        let viewport_id = Self::viewport_id(WindowId::Doom);
        if state.focus_requested {
            state.focus_requested = false;
            raise(ctx, viewport_id);
        }

        let mut close_requested = false;
        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                // No project context: DOOM.666 is not a project artifact (D6).
                .with_title("DOOM.666")
                // 3× the engine's 320×200 frame, stretched to 4:3 the way the
                // original was.
                .with_inner_size([960.0, 720.0]),
            |ui, _class| {
                doom_window::show(ui, state);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested())
                    || close_window_pressed(ui.ctx());
            },
        );

        if close_requested {
            self.windows.close_doom();
        }
    }

    fn show_graph_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
        let Some(session) = self.windows.graph.as_mut() else {
            return;
        };
        session.tick(&self.graph_service);

        let viewport_id = Self::viewport_id(WindowId::Graph);
        if session.focus_requested {
            session.focus_requested = false;
            raise(ctx, viewport_id);
        }

        let title = window_title(self.current_project.as_ref(), "Signal Graph");
        let mut close_requested = false;
        let mut menu_command = None;
        ctx.show_viewport_immediate(
            viewport_id,
            ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([1040.0, 760.0]),
            |ui, _class| {
                menu_command = Self::window_menu_bar(ui, context, WindowId::Graph);
                graph_window::show(ui, session);
                close_requested = ui.ctx().input(|i| i.viewport().close_requested())
                    || close_window_pressed(ui.ctx());
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
        // Keeps the splash timer, playhead and graph worker ticking with no user
        // input — scheduled on a window that is actually **on screen**, never on
        // the hidden root.
        //
        // eframe paints an invisible or minimised window by calling
        // `run_ui_and_paint` straight from `new_events` (`run.rs:222`) — the one
        // handler it does not wrap in `with_event_loop_context`. A tick scheduled
        // on the hidden root therefore produces a frame with no event-loop
        // thread-local, and on such a frame `show_viewport_immediate` cannot
        // create its window (`wgpu_integration.rs:1081`) and egui asserts with
        // "the user callback was never called". That is the crash class
        // `prepare_simulator` works around for one case; scheduling the tick on a
        // visible viewport removes it for all of them, because the tick then
        // arrives as `window.request_redraw()` → `RedrawRequested` →
        // `window_event`, which *is* wrapped. Immediate viewports all repaint in
        // one pass, so a single visible window keeps the whole tree ticking.
        ctx.request_repaint_after_for(TICK_INTERVAL, self.ticking_viewport(&ctx));

        // One context per frame, shared by the native bar, every per-window bar
        // and the close handling — so all of them agree on what has focus and
        // what is enabled.
        let context = self.menu_context(&ctx);
        self.focused = context.focused;
        let native_commands = self.sync_native_menu(&context);

        match self.screen {
            Screen::Splash => self.show_splash(ui, &ctx),
            Screen::Initial => self.show_initial_viewport(&ctx),
            Screen::Editor => {
                self.show_editor_viewport(&ctx, &context);
                // Beside the Editor, for File ▸ Open Project (D3).
                if self.initial_open {
                    self.show_initial_viewport(&ctx);
                }
                self.show_simulator_viewport(&ctx, &context);
                self.show_graph_viewport(&ctx, &context);
                self.show_doom_viewport(&ctx);
            }
        }

        // The native bar's clicks run *after* the viewports, for the reason
        // `prepare_simulator` documents: creating a viewport window is only legal
        // on a frame with eframe's event-loop thread-local set, and a frame that
        // has just painted one is provably such a frame. Draining these at the
        // top of `ui` instead would eventually assert.
        for command in native_commands {
            self.run_menu_command(command);
        }

        // Closing the (hidden) root quits the app; child viewports die with it.
        if self.quit_requested || ctx.input(|i| i.viewport().close_requested()) {
            self.quit_requested = false;
            self.windows.close_all();
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }
}

/// Brings a viewport to the front, **un-minimising it first**.
///
/// `Focus` alone does not restore a minimised window, and with the root hidden
/// there is no Dock icon to click and no Bring All to Front to fall back on — so
/// a window minimised from its title bar would otherwise be gone for good. The
/// Window menu is the way back, and this is what makes it work.
fn raise(ctx: &egui::Context, viewport_id: ViewportId) {
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
}

/// Whether ⌘W / Ctrl+W was pressed in a secondary window, which closes it (D5).
///
/// Only ever reached off macOS: there the chord is a menu accelerator, so the
/// menu bar consumes the key before the window is offered it, and the click
/// arrives as `MenuCommand::CloseWindow` instead.
fn close_window_pressed(ctx: &egui::Context) -> bool {
    let chord = menu_controller::shortcut(menu::CLOSE_TAB);
    ctx.input_mut(|input| input.consume_shortcut(&chord))
}

/// The Editor's title: the active document first, then the project, which is the
/// macOS convention (D6). The project alone when nothing is open.
fn editor_title(state: &EditorState) -> String {
    match state.tabs.get(state.active_tab) {
        Some(tab) => format!("{} — {}", tab.name, state.project_name),
        None => state.project_name.clone(),
    }
}

/// A secondary window's title: `<project> — <window>`, or just the window name
/// before a project is open.
fn window_title(project: Option<&RecentProject>, window: &str) -> String {
    match project {
        Some(project) => format!("{} — {}", project.name, window),
        None => window.to_string(),
    }
}

/// The About box (S5): an egui modal rather than a native panel, so all three
/// platforms show the same thing. Returns whether it was dismissed.
fn about_dialog(ctx: &egui::Context) -> bool {
    let mut dismissed = false;
    egui::Modal::new(egui::Id::new("about_headroomlab")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("HeadroomLab")
                    .font(theme::title_font())
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .font(theme::small_font())
                    .color(theme::MUTED_ON_DARK),
            );
            ui.add_space(10.0);
            ui.label("Audition guitar-pedal DSP effects on real audio.");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Effects are C-ABI shared libraries exporting hl_create, hl_process, \
                     hl_set_knob, hl_set_switch, hl_set_footswitch and hl_destroy.",
                )
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
            );
            ui.add_space(14.0);
            if ui.button("OK").clicked() {
                dismissed = true;
            }
            ui.add_space(4.0);
        });
    });
    dismissed
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
