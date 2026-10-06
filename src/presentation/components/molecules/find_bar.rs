use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::presentation::theme;

pub struct FindState {
    pub query: String,
    pub current: usize,
    pub focus: bool,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            query: String::new(),
            current: 0,
            focus: true,
        }
    }
}

#[derive(Default)]
pub struct FindEvents {
    pub changed: bool,
    pub next: bool,
    pub previous: bool,
    pub close: bool,
}

pub fn find_bar(ui: &mut egui::Ui, state: &mut FindState, match_count: usize) -> FindEvents {
    let mut events = FindEvents::default();

    egui::Frame::new()
        .fill(theme::INSET_SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::INSET_BORDER))
        .corner_radius(egui::CornerRadius::same(theme::CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(ph::MAGNIFYING_GLASS)
                        .font(egui::FontId::proportional(theme::FONT_BODY))
                        .color(theme::MUTED_ON_DARK),
                );

                let field = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .desired_width(200.0)
                        .hint_text("Find"),
                );
                if state.focus {
                    field.request_focus();
                    state.focus = false;
                }
                if field.changed() {
                    events.changed = true;
                }
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    events.next = true;
                    field.request_focus();
                }

                let summary = if state.query.is_empty() {
                    String::new()
                } else if match_count == 0 {
                    "No results".to_owned()
                } else {
                    format!("{} of {match_count}", state.current + 1)
                };
                ui.label(
                    RichText::new(summary)
                        .font(theme::small_font())
                        .color(theme::MUTED_ON_DARK),
                );

                let enabled = match_count > 0;
                if ui
                    .add_enabled(enabled, theme::selectable_button(ph::CARET_UP, false))
                    .clicked()
                {
                    events.previous = true;
                }
                if ui
                    .add_enabled(enabled, theme::selectable_button(ph::CARET_DOWN, false))
                    .clicked()
                {
                    events.next = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(theme::selectable_button(ph::X, false)).clicked() {
                        events.close = true;
                    }
                });
            });
        });

    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        events.close = true;
    }

    events
}
