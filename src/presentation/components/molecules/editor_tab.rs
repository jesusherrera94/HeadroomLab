//! One tab in the editor's tab strip: kind icon, file name, an unsaved ● dot,
//! and a close (×) button that appears on hover or when active. The active tab
//! is filled to stand out. Returns which parts were clicked.

use eframe::egui::{self, CornerRadius, RichText};
use egui_phosphor::regular as ph;

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::editor_controller::EditorTab;
use crate::presentation::theme;

#[derive(Default)]
pub struct EditorTabResponse {
    /// The tab body was clicked (activate it).
    pub clicked: bool,
    /// The close (×) button was clicked.
    pub close_clicked: bool,
}

pub fn editor_tab(ui: &mut egui::Ui, tab: &EditorTab, active: bool) -> EditorTabResponse {
    let mut response = EditorTabResponse::default();

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
                file_icon(ui, tab.icon);
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
                if tab.unsaved() {
                    ui.label(
                        RichText::new("●")
                            .font(theme::small_font())
                            .color(theme::LABEL_ON_DARK),
                    );
                }
                // Close button, drawn as a small × label made interactive.
                let close = ui.add(
                    egui::Label::new(
                        RichText::new(ph::X)
                            .font(egui::FontId::proportional(theme::FONT_BODY))
                            .color(theme::MUTED_ON_DARK),
                    )
                    .sense(egui::Sense::click()),
                );
                if close.hovered() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                }
                if close.clicked() {
                    response.close_clicked = true;
                }
            });
        });

    let body = ui.interact(inner.response.rect, inner.response.id, egui::Sense::click());
    if body.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    // A click anywhere but the × activates the tab.
    if body.clicked() && !response.close_clicked {
        response.clicked = true;
    }

    response
}
