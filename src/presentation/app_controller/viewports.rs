//! Drawing the editor, simulator, graph and DOOM viewports each frame and
//! acting on what they report.

use eframe::egui::{self, ViewportBuilder, ViewportCommand};

use super::HeadroomApp;
use super::dialogs::{about_dialog, notice_dialog};
use super::focus::{close_window_pressed, raise};
use crate::domain::menu::{MenuContext, WindowId};
use crate::domain::project::RecentProject;
use crate::domain::terminal::BuildKind;
use crate::presentation::components::organisms::code_pane;
use crate::presentation::editor_controller::{self, EditorState};
use crate::presentation::simulation_controller;
use crate::presentation::windows::{doom_window, editor_window, graph_window, simulator_window};

impl HeadroomApp {
    pub(super) fn show_editor_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
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

    pub(super) fn show_simulator_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
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

    pub(super) fn show_doom_viewport(&mut self, ctx: &egui::Context) {
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

    pub(super) fn show_graph_viewport(&mut self, ctx: &egui::Context, context: &MenuContext) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_windows_are_titled_after_the_project_when_there_is_one() {
        let project = RecentProject::new("My Fuzz", "/tmp/my-fuzz");
        assert_eq!(
            window_title(Some(&project), "Signal Graph"),
            "My Fuzz — Signal Graph"
        );
        assert_eq!(window_title(None, "Signal Graph"), "Signal Graph");
    }
}
