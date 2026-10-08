mod grid;
mod header;
mod input;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::domain::terminal::{BuildStatus, GIT_FOR_WINDOWS_URL, TerminalSize, grid_size};
use crate::presentation::terminal_controller::{
    self, SessionKind, TerminalRequests, TerminalState,
};
use crate::presentation::theme;

use grid::paint_grid;
use header::header;
pub use input::key_bytes;
use input::{handle_keys, handle_mouse, handle_scroll};

fn focus_id() -> egui::Id {
    egui::Id::new("terminal_grid")
}

pub fn has_focus(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.has_focus(focus_id()))
}

pub fn terminal_panel(ui: &mut egui::Ui, state: &mut TerminalState) -> TerminalRequests {
    let mut requests = terminal_controller::tick(state);

    header(ui, state);

    egui::Frame::new()
        .fill(theme::PLOT_FRAME_BACKGROUND)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height().max(0.0));
            body(ui, state, &mut requests);
        });

    requests
}

fn body(ui: &mut egui::Ui, state: &mut TerminalState, requests: &mut TerminalRequests) {
    let (cell_width, cell_height) = cell_metrics(ui);
    terminal_controller::ensure_open(state);

    if state.missing_git_bash {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} `make` needs Git for Windows.",
                    ph::WARNING_CIRCLE
                ))
                .font(theme::small_font())
                .color(theme::UNSAVED_DOT),
            );
            ui.hyperlink_to(
                RichText::new("Install it").font(theme::small_font()),
                GIT_FOR_WINDOWS_URL,
            );
        });
        ui.add_space(4.0);
    }

    let Some(session) = state.active_session() else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("No terminal session — press + to open one")
                    .font(egui::FontId::monospace(theme::FONT_BODY))
                    .color(theme::MUTED_ON_DARK),
            );
        });
        return;
    };

    if let Some(error) = session.error.clone() {
        let id = session.id;
        notice(ui, &error, theme::ERROR_COLOR, "Retry", || {
            terminal_controller::restart(state, id)
        });
        return;
    }

    if let Some(text) = session.exit_notice() {
        let failed = matches!(
            (session.kind, session.status),
            (SessionKind::Build(_), Some(BuildStatus::Failed(_)))
        );
        let id = session.id;
        let restart = exit_strip(ui, &text, failed);
        if restart {
            terminal_controller::restart(state, id);
            return;
        }
    }

    let available = ui.available_size();
    let (cols, rows) = grid_size(available.x, available.y, cell_width, cell_height);
    terminal_controller::resize(
        state,
        TerminalSize {
            cols,
            rows,
            cell_width: cell_width.round() as u16,
            cell_height: cell_height.round() as u16,
        },
    );

    let Some(snapshot) = terminal_controller::active_snapshot(state) else {
        return;
    };

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ui.available_height()),
        egui::Sense::hover(),
    );
    let response = ui.interact(rect, focus_id(), egui::Sense::click_and_drag());
    let origin = rect.min;

    paint_grid(ui, &snapshot, origin, cell_width, cell_height);

    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }

    handle_mouse(ui, state, &response, origin, cell_width, cell_height);

    if has_focus(ui.ctx()) {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                focus_id(),
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
        handle_keys(ui, state, requests);
    }

    handle_scroll(ui, state, &response, cell_height);
}

fn exit_strip(ui: &mut egui::Ui, message: &str, failed: bool) -> bool {
    let mut restart = false;
    ui.horizontal(|ui| {
        let color = if failed {
            theme::ERROR_COLOR
        } else {
            theme::MUTED_ON_DARK
        };
        ui.label(
            RichText::new(message)
                .font(egui::FontId::monospace(theme::FONT_BODY))
                .color(color),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            restart = ui.add(theme::selectable_button("Restart", false)).clicked();
        });
    });
    ui.add_space(4.0);
    restart
}

fn notice(
    ui: &mut egui::Ui,
    message: &str,
    color: egui::Color32,
    action: &str,
    mut on_action: impl FnMut(),
) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new(message)
                .font(egui::FontId::monospace(theme::FONT_BODY))
                .color(color),
        );
        ui.add_space(6.0);
        if ui.add(theme::selectable_button(action, false)).clicked() {
            on_action();
        }
    });
}

fn cell_metrics(ui: &egui::Ui) -> (f32, f32) {
    let font = egui::FontId::monospace(theme::FONT_BODY);
    ui.fonts_mut(|fonts| {
        let width = fonts.glyph_width(&font, 'M');
        let height = fonts.row_height(&font);
        (width.max(1.0), height.max(1.0))
    })
}
