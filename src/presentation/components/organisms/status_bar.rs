use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

use crate::domain::diagnostics::{Counts, summary};
use crate::domain::terminal::{BuildKind, BuildStatus};
use crate::domain::text_document::{DocumentContent, language_label};
use crate::presentation::editor_controller::{EditorTab, save_shortcut_label};
use crate::presentation::theme;

const DOT_RADIUS: f32 = 3.5;
const DOT_SLOT: f32 = 12.0;

pub struct StatusInfo<'a> {
    pub project_name: &'a str,
    pub tab: Option<&'a EditorTab>,
    pub cursor: Option<(usize, usize)>,
    pub build: Option<(BuildKind, BuildStatus)>,
    pub diagnostics: Counts,
}

#[derive(Default)]
pub struct StatusBarEvents {
    pub save: bool,
}

pub fn status_bar(ui: &mut egui::Ui, info: StatusInfo<'_>) -> StatusBarEvents {
    let mut events = StatusBarEvents::default();

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        segment(
            ui,
            &format!("{} main — {}", ph::GIT_BRANCH, info.project_name),
        );

        if let Some(tab) = info.tab
            && tab.unsaved()
        {
            events.save = unsaved_segment(ui, tab);
        }

        if let Some((kind, status)) = info.build {
            let color = match status {
                BuildStatus::Running => theme::MUTED_ON_DARK,
                BuildStatus::Succeeded => theme::LABEL_ON_DARK,
                BuildStatus::Failed(_) => theme::ERROR_COLOR,
            };
            ui.label(
                RichText::new(status.summary(kind))
                    .font(theme::small_font())
                    .color(color),
            );
            ui.add_space(12.0);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            segment(ui, "UTF-8");
            segment(ui, "Spaces: 2");

            let (line, col) = info.cursor.unwrap_or((1, 1));
            segment(ui, &format!("Ln {line}, Col {col}"));

            if let Some(text) = summary(info.diagnostics) {
                let color = if info.diagnostics.errors > 0 {
                    theme::DIAGNOSTIC_ERROR
                } else {
                    theme::DIAGNOSTIC_WARNING
                };
                ui.label(RichText::new(text).font(theme::small_font()).color(color));
                ui.add_space(10.0);
            }

            if let Some(tab) = info.tab {
                if matches!(
                    tab.content,
                    DocumentContent::Text {
                        highlight: false,
                        ..
                    }
                ) {
                    segment(ui, "Highlighting off");
                }
                segment(ui, language_label(tab.language));
            }
        });
    });

    events
}

fn unsaved_segment(ui: &mut egui::Ui, tab: &EditorTab) -> bool {
    let inner = ui.horizontal(|ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(DOT_SLOT, DOT_RADIUS * 2.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), DOT_RADIUS, theme::UNSAVED_DOT);
        ui.label(
            RichText::new(format!("{} — {} to save", tab.name, save_shortcut_label()))
                .font(theme::small_font())
                .color(theme::UNSAVED_DOT),
        );
    });
    ui.add_space(10.0);

    let id = ui.make_persistent_id("status_bar_unsaved");
    let response = ui
        .interact(inner.response.rect, id, egui::Sense::click())
        .on_hover_text("Save this file");
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn segment(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::small_font())
            .color(theme::MUTED_ON_DARK),
    );
    ui.add_space(10.0);
}
