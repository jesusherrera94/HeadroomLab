//! One tab in the editor's tab strip: kind icon, file name, and an unsaved ●
//! dot. The active tab is filled to stand out. Returns `true` when clicked.

use eframe::egui::{self, CornerRadius, RichText};

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::editor_controller::EditorTab;
use crate::presentation::theme;

pub fn editor_tab(ui: &mut egui::Ui, tab: &EditorTab, active: bool) -> bool {
    let fill = if active {
        theme::TAB_ACTIVE_BACKGROUND
    } else {
        egui::Color32::TRANSPARENT
    };

    let inner = egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(theme::CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                file_icon(ui, tab.kind);
                let name_color = if active {
                    egui::Color32::WHITE
                } else {
                    theme::MUTED_ON_DARK
                };
                ui.label(
                    RichText::new(&tab.name)
                        .font(theme::body_font())
                        .color(name_color),
                );
                if tab.unsaved {
                    ui.label(
                        RichText::new("●")
                            .font(theme::small_font())
                            .color(theme::LABEL_ON_DARK),
                    );
                }
            });
        });

    let response = ui.interact(inner.response.rect, inner.response.id, egui::Sense::click());
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    response.clicked()
}
