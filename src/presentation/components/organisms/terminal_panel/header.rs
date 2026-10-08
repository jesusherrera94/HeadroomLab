//! The panel's header strip: session tabs, new-session and clear buttons.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::domain::terminal::BuildStatus;
use crate::presentation::terminal_controller::{
    self, Session, SessionId, SessionKind, TerminalState,
};
use crate::presentation::theme;

pub(super) fn header(ui: &mut egui::Ui, state: &mut TerminalState) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("TERMINAL")
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
        );
        ui.add_space(8.0);

        let mut activate: Option<SessionId> = None;
        let mut close: Option<SessionId> = None;

        egui::ScrollArea::horizontal()
            .id_salt("terminal_tabs")
            .max_width(ui.available_width() - 60.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, session) in state.sessions.iter().enumerate() {
                        let active = index == state.active;
                        let label = tab_label(session);

                        if ui
                            .add(theme::selectable_button(&label, active))
                            .on_hover_text(session.command.program.clone())
                            .clicked()
                        {
                            activate = Some(session.id);
                        }
                        if ui
                            .add(theme::selectable_button(ph::X, false))
                            .on_hover_text("Close session")
                            .clicked()
                        {
                            close = Some(session.id);
                        }
                        ui.add_space(6.0);
                    }
                });
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(theme::selectable_button(ph::PLUS, false))
                .on_hover_text("New session")
                .clicked()
            {
                terminal_controller::open_shell(state);
            }
            if ui
                .add_enabled(
                    state.active_session().is_some(),
                    theme::selectable_button(ph::TRASH, false),
                )
                .on_hover_text("Clear")
                .clicked()
            {
                terminal_controller::clear_active(state);
            }
        });

        if let Some(id) = activate {
            terminal_controller::activate(state, id);
        }
        if let Some(id) = close {
            terminal_controller::close(state, id);
        }
    });
    ui.add_space(2.0);
}

fn tab_label(session: &Session) -> String {
    let marker = match (session.kind, session.status) {
        (SessionKind::Build(_), Some(BuildStatus::Running)) => Some(ph::SPINNER_GAP),
        (SessionKind::Build(_), Some(BuildStatus::Succeeded)) => Some(ph::CHECK_CIRCLE),
        (SessionKind::Build(_), Some(BuildStatus::Failed(_))) => Some(ph::X_CIRCLE),
        _ => None,
    };
    match marker {
        Some(marker) => format!("{marker} {}", session.title),
        None => session.title.clone(),
    }
}
