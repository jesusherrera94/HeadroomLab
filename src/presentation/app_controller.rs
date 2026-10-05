use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};

use crate::application::file_system_service::FileSystemService;
use crate::application::graph_service::GraphService;
use crate::application::ports::UpdaterPort;
use crate::application::ports::{
    ClipboardPort, DoomPort, FileWatcherPort, ProjectFileSystemPort, ProjectGeneratorPort,
    TerminalPort,
};
use crate::application::recent_projects_service::RecentProjectsService;
use crate::application::simulator_service::SimulatorService;
use crate::application::update_service::{Trigger, UpdateService};
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

const TICK_INTERVAL: Duration = Duration::from_millis(100);
const SPLASH_DURATION: Duration = Duration::from_millis(600);
const CLIPBOARD_POLL: Duration = Duration::from_millis(500);

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
    updater: Arc<dyn UpdaterPort>,
    updates: UpdateService,
    windows: WindowManager,
    simulator_awaiting_build: bool,

    screen: Screen,
    splash_started: Instant,
    initial: InitialState,
    current_project: Option<RecentProject>,
    editor: Option<EditorState>,

    initial_open: bool,
    focused: WindowId,
    quit_requested: bool,
    about_open: bool,
    update_notice: Option<String>,
    manual_check_running: bool,
    pending_restart: bool,
    update_started: bool,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::presentation::native_menu::NativeMenu>,
    other_widget_focused: bool,
    clipboard_ready: bool,
    clipboard_checked: Instant,
}

impl HeadroomApp {
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
        updates: UpdateService,
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
            updater: updates.updater(),
            updates,
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
            update_notice: None,
            manual_check_running: false,
            pending_restart: false,
            update_started: false,
            #[cfg(target_os = "macos")]
            native_menu: None,
            other_widget_focused: false,
            clipboard_ready: false,
            clipboard_checked: Instant::now() - CLIPBOARD_POLL,
        }
    }

    fn ticking_viewport(&self, ctx: &egui::Context) -> ViewportId {
        let preferred = match self.screen {
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
        let fallback = [
            (WindowId::Simulator, self.windows.simulator.is_some()),
            (WindowId::Graph, self.windows.graph.is_some()),
            (WindowId::Doom, self.windows.doom.is_some()),
            (WindowId::Initial, self.initial_open),
        ]
        .into_iter()
        .find(|(window, open)| *open && on_screen(*window))
        .map(|(window, _)| window);

        Self::viewport_id(fallback.unwrap_or(preferred))
    }


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

    fn menu_context(&mut self, ctx: &egui::Context) -> MenuContext {
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
            updates_available: self.updates.is_enabled(),
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
                context.has_selection =
                    egui::text_edit::TextEditState::load(ctx, code_pane::editor_id(tab.id))
                        .and_then(|state| state.cursor.char_range())
                        .is_some_and(|range| range.primary.index != range.secondary.index);
            }
        }

        context
    }

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
        if requests.show_about {
            self.about_open = true;
            match self.editor.as_mut() {
                Some(editor) => editor.focus_requested = true,
                None => self.initial.focus_requested = true,
            }
        }
        if requests.check_for_updates {
            self.manual_check_running = true;
            self.update_notice = Some("Checking for updates…".to_owned());
            self.updates.check_manually();
        }
        if requests.open_help
            && let Err(e) = opener::open_browser(crate::config::HELP_URL)
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
        if requests.quit {
            self.request_quit();
        }
        if requests.close_window {
            self.close_focused_window();
        }
    }

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

    fn close_focused_window(&mut self) {
        match self.focused {
            WindowId::Simulator => self.windows.close_simulator(),
            WindowId::Graph => self.windows.close_graph(),
            WindowId::Doom => self.windows.close_doom(),
            WindowId::Initial if self.initial_open => self.initial_open = false,
            _ => self.request_quit(),
        }
    }

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

    fn show_splash(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let events = splash_window::show(ui, self.updates.state());
        if events.continue_anyway {
            self.updates.acknowledge_failure();
        }

        let shown_long_enough = self.splash_started.elapsed() >= SPLASH_DURATION;
        if self.updates.state().is_settled() && shown_long_enough {
            self.screen = Screen::Initial;
            self.initial.focus_requested = true;
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(false));
        }
    }

    fn handle_update_outcome(&mut self) {
        if self.updates.trigger() == Trigger::Manual
            && self.updates.pending_release().is_some()
            && !self.updates.is_running()
        {
            let version = self
                .updates
                .pending_release()
                .map(|r| r.version.clone())
                .unwrap_or_default();
            self.update_notice = Some(format!("Downloading v{version}…"));
            self.manual_check_running = false;
            self.updates.install_found();
        }

        if let Some(version) = self.updates.take_restart_request() {
            match self.screen {
                Screen::Splash | Screen::Initial => self.restart_into_update(),
                Screen::Editor => {
                    self.pending_restart = true;
                    self.update_notice =
                        Some(format!("v{version} is installed. Restarting HeadroomLab…"));
                    self.request_quit();
                }
            }
        }

        if self.manual_check_running
            && self.updates.trigger() == Trigger::Manual
            && !self.updates.is_running()
            && self.updates.pending_release().is_none()
        {
            self.manual_check_running = false;
            if self.updates.state().is_settled() {
                self.update_notice = Some(format!(
                    "HeadroomLab v{} is the latest version.",
                    env!("CARGO_PKG_VERSION")
                ));
            }
        }

        if self.updates.state().needs_acknowledgement()
            && matches!(self.screen, Screen::Editor)
            && let Some(editor) = self.editor.as_mut()
        {
            editor.explorer.error = Some(self.updates.state().status_line());
            self.updates.acknowledge_failure();
            self.manual_check_running = false;
            self.update_notice = None;
        }
    }

    fn restart_into_update(&mut self) {
        match self.updater.restart() {
            Ok(never) => match never {},
            Err(e) => {
                eprintln!("[updater] could not restart: {e}");
                if let Some(editor) = self.editor.as_mut() {
                    editor.explorer.error = Some(format!(
                        "The update was installed but the app could not restart: {e}\n\n                         Quit and reopen HeadroomLab to use the new version."
                    ));
                }
            }
        }
    }

    fn show_initial_viewport(&mut self, ctx: &egui::Context) {
        let viewport_id = Self::viewport_id(WindowId::Initial);
        if self.initial.focus_requested {
            self.initial.focus_requested = false;
            raise(ctx, viewport_id);
        }

        let recents = self.recents.borrow().list().to_vec();
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
            if self.initial_open {
                self.initial_open = false;
                self.initial.close_modal();
            } else {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            }
        }
    }

    fn open_initial_window(&mut self, mode: OpenInitial) {
        self.initial_open = true;
        self.initial.focus_requested = true;
        match mode {
            OpenInitial::CreateModal => self.initial.open_create_modal(),
            OpenInitial::Picker => self.initial.close_modal(),
        }
    }

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

    fn request_project(&mut self, project: RecentProject) {
        let guarded = self.editor.as_mut().is_some_and(|editor| {
            editor_controller::request_switch_project(editor, project.clone())
        });
        if guarded {
            if let Some(editor) = self.editor.as_mut() {
                editor.focus_requested = true;
            }
            return;
        }
        self.open_project(project);
    }

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
    }

    fn show_editor_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
        let Some(state) = self.editor.as_mut() else {
            return;
        };

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
        let mut other_widget_focused = false;
        let notice = self.update_notice.clone();
        let notice_dismissable = !self.updates.state().is_working();
        let mut notice_dismissed = false;

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
                } else if let Some(text) = notice.as_deref() {
                    notice_dismissed = notice_dialog(ui.ctx(), text, notice_dismissable);
                }

                let code_area = state
                    .tabs
                    .get(state.active_tab)
                    .map(|tab| code_pane::editor_id(tab.id));
                other_widget_focused = ui
                    .ctx()
                    .memory(|memory| memory.focused())
                    .is_some_and(|focused| Some(focused) != code_area);

                for tab in editor_controller::take_discarded_tabs(state) {
                    code_pane::forget_editor_state(ui.ctx(), tab);
                }

                if close_requested && editor_controller::request_quit(state) {
                    ui.ctx().send_viewport_cmd(ViewportCommand::CancelClose);
                    close_requested = false;
                }
            },
        );

        self.other_widget_focused = other_widget_focused;
        if about_dismissed {
            self.about_open = false;
        }
        if notice_dismissed {
            self.update_notice = None;
        }
        if notice_dismissed {
            self.update_notice = None;
        }

        if requests.build_run {
            self.run_build(BuildKind::Dylib);
        } else if requests.compile {
            self.run_build(BuildKind::Firmware);
        }

        if requests.open_emulator {
            self.launch_simulator();
        }
        if requests.open_doom {
            self.windows.open_doom(&self.doom);
        }
        if requests.build_run {
            self.prepare_simulator();
        }
        if requests.reload_plugin {
            self.reveal_simulator();
        }
        if requests.build_failed {
            self.discard_pending_simulator();
        }
        if requests.quit_requested {
            self.request_quit();
        }
        if let Some(project) = requests.switch_project {
            self.open_project(project);
        }
        if close_requested || requests.quit_confirmed {
            self.quit_requested = true;
        }

        if let Some(command) = menu_command {
            self.run_menu_command(command);
        }
    }

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

    fn prepare_simulator(&mut self) {
        if self.windows.simulator.is_some() {
            return;
        }
        self.windows.open_simulator();
        self.simulator_awaiting_build = true;
    }

    fn reveal_simulator(&mut self) {
        self.simulator_awaiting_build = false;
        let Some(state) = self.windows.simulator.as_mut() else {
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

    fn discard_pending_simulator(&mut self) {
        if self.simulator_awaiting_build {
            self.simulator_awaiting_build = false;
            self.windows.close_simulator();
        }
    }

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
                .with_title("DOOM.666")
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
        ctx.request_repaint_after_for(TICK_INTERVAL, self.ticking_viewport(&ctx));

        if !std::mem::replace(&mut self.update_started, true) {
            self.updates.start_at_launch();
        }

        self.updates.tick();
        self.handle_update_outcome();

        let context = self.menu_context(&ctx);
        self.focused = context.focused;
        let native_commands = self.sync_native_menu(&context);

        match self.screen {
            Screen::Splash => self.show_splash(ui, &ctx),
            Screen::Initial => self.show_initial_viewport(&ctx),
            Screen::Editor => {
                self.show_editor_viewport(&ctx, &context);
                if self.initial_open {
                    self.show_initial_viewport(&ctx);
                }
                self.show_simulator_viewport(&ctx, &context);
                self.show_graph_viewport(&ctx, &context);
                self.show_doom_viewport(&ctx);
            }
        }

        for command in native_commands {
            self.run_menu_command(command);
        }

        if self.quit_requested || ctx.input(|i| i.viewport().close_requested()) {
            self.quit_requested = false;
            self.windows.close_all();
            if std::mem::take(&mut self.pending_restart) {
                self.restart_into_update();
            }
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }
}

fn raise(ctx: &egui::Context, viewport_id: ViewportId) {
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(viewport_id, ViewportCommand::Focus);
}

fn close_window_pressed(ctx: &egui::Context) -> bool {
    let chord = menu_controller::shortcut(menu::CLOSE_TAB);
    ctx.input_mut(|input| input.consume_shortcut(&chord))
}

fn editor_title(state: &EditorState) -> String {
    match state.tabs.get(state.active_tab) {
        Some(tab) => format!("{} — {}", tab.name, state.project_name),
        None => state.project_name.clone(),
    }
}

fn window_title(project: Option<&RecentProject>, window: &str) -> String {
    match project {
        Some(project) => format!("{} — {}", project.name, window),
        None => window.to_string(),
    }
}

fn notice_dialog(ctx: &egui::Context, text: &str, dismissable: bool) -> bool {
    let mut dismissed = false;
    egui::Modal::new(egui::Id::new("update_notice")).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            ui.label(RichTextExt::body(text));
            ui.add_space(14.0);
            if ui
                .add_enabled(dismissable, egui::Button::new("OK"))
                .clicked()
            {
                dismissed = true;
            }
            ui.add_space(4.0);
        });
    });
    dismissed
}

struct RichTextExt;

impl RichTextExt {
    fn body(text: &str) -> egui::RichText {
        egui::RichText::new(text).font(theme::body_font())
    }
}

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
