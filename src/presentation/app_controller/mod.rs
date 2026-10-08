mod dialogs;
mod focus;
mod menus;
mod projects;
mod simulator;
mod updates;
mod viewports;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportCommand, ViewportId};

use crate::application::file_system_service::FileSystemService;
use crate::application::graph_service::GraphService;
use crate::application::ports::UpdaterPort;
use crate::application::ports::{
    ClipboardPort, DoomPort, FileWatcherPort, ProjectFileSystemPort, ProjectGeneratorPort,
    TerminalPort,
};
use crate::application::recent_projects_service::RecentProjectsService;
use crate::application::simulator_service::SimulatorService;
use crate::application::update_service::UpdateService;
use crate::domain::menu::WindowId;
use crate::domain::project::RecentProject;
use crate::presentation::editor_controller::EditorState;
use crate::presentation::initial_controller::InitialState;
use crate::presentation::window_manager::WindowManager;

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
