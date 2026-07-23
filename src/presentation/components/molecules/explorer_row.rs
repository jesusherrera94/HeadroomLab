//! One row in the (mocked) explorer tree: indentation by depth, a kind icon and
//! the file/folder name, with a hover highlight. Returns `true` when clicked.
//! Mirrors the interaction pattern of `recent_project_item`.

use eframe::egui::{self, CornerRadius, RichText};

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::editor_controller::{ExplorerNode, FileKind};
use crate::presentation::theme;

/// Indentation applied per tree depth level.
const INDENT: f32 = 14.0;

pub fn explorer_row(ui: &mut egui::Ui, node: &ExplorerNode) -> bool {
    // Reserve a shape slot for the hover highlight painted behind the row.
    let bg = ui.painter().add(egui::Shape::Noop);

    let frame = egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 3));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add_space(node.depth as f32 * INDENT);
            file_icon(ui, node.kind);
            let color = if node.kind == FileKind::Folder {
                theme::LABEL_ON_DARK
            } else {
                theme::MUTED_ON_DARK
            };
            ui.label(
                RichText::new(&node.name)
                    .font(theme::body_font())
                    .color(color),
            );
        });
    });

    let response = ui.interact(inner.response.rect, inner.response.id, egui::Sense::click());
    if response.hovered() {
        ui.painter().set(
            bg,
            egui::Shape::rect_filled(
                response.rect,
                CornerRadius::same(theme::CORNER_RADIUS),
                theme::ROW_HOVER,
            ),
        );
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }

    response.clicked()
}
