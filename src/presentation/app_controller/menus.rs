//! The menu bar: building its context, syncing the native macOS bar, and
//! dispatching the commands it emits.

use std::time::Instant;

use eframe::egui;

use super::{CLIPBOARD_POLL, HeadroomApp};
use crate::domain::menu::{
    MenuCommand, MenuContext, MenuModel, MenuSurface, TransportCommand, WindowId,
};
use crate::domain::terminal::BuildKind;
use crate::domain::text_document::line_comment;
use crate::presentation::components::organisms::{code_pane, menu_bar};
use crate::presentation::editor_controller;
use crate::presentation::menu_controller;
use crate::presentation::simulation_controller;
use crate::presentation::terminal_controller;
use crate::presentation::windows::simulator_window::SimulatorViewEvents;

impl HeadroomApp {
    pub(super) fn menu_context(&mut self, ctx: &egui::Context) -> MenuContext {
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
    pub(super) fn sync_native_menu(&mut self, context: &MenuContext) -> Vec<MenuCommand> {
        use crate::presentation::native_menu::NativeMenu;

        let menu = self
            .native_menu
            .get_or_insert_with(|| NativeMenu::install(context));
        menu.sync(context);
        menu.poll()
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn sync_native_menu(&mut self, _context: &MenuContext) -> Vec<MenuCommand> {
        Vec::new()
    }

    pub(super) fn window_menu_bar(
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

    pub(super) fn run_menu_command(&mut self, command: MenuCommand) {
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

    pub(super) fn request_quit(&mut self) {
        let guarded = self
            .editor
            .as_mut()
            .is_some_and(editor_controller::request_quit);
        if !guarded {
            self.quit_requested = true;
        }
    }

    pub(super) fn run_build(&mut self, kind: BuildKind) {
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
}
