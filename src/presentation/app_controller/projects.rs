//! The project picker window and opening, creating or switching projects.

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};

use super::dialogs::about_dialog;
use super::focus::raise;
use super::{HeadroomApp, Screen};
use crate::domain::menu::WindowId;
use crate::domain::project::RecentProject;
use crate::presentation::editor_controller::{self, EditorState};
use crate::presentation::initial_controller::{self, ProjectIntent};
use crate::presentation::menu_controller::OpenInitial;
use crate::presentation::windows::initial_window;

impl HeadroomApp {
    pub(super) fn show_initial_viewport(&mut self, ctx: &egui::Context) {
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
                .with_inner_size([460.0, 480.0])
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

    pub(super) fn open_initial_window(&mut self, mode: OpenInitial) {
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

    pub(super) fn request_project(&mut self, project: RecentProject) {
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

    pub(super) fn open_project(&mut self, project: RecentProject) {
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
}
