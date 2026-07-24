//! One row in the explorer tree: a collapse caret (folders only), a kind icon
//! and the entry name, with hover and selection highlights. Returns the click
//! `Response` so the organism can toggle folders / select files. Indentation is
//! handled by the caller's `CollapsingState` body, not here.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, RichText, Sense};
use egui_phosphor::regular as ph;

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::editor_controller::{NodeIcon, TreeNode};
use crate::presentation::theme;

/// Width reserved for the collapse caret so folder and file icons align.
const CARET_WIDTH: f32 = 14.0;
const CARET_SIZE: f32 = 12.0;

/// Renders `node` as a row. `open` is `Some(is_open)` for directories (drives
/// the caret and folder-open icon) and `None` for files. Returns the row's
/// click response.
pub fn explorer_row(
    ui: &mut egui::Ui,
    node: &TreeNode,
    open: Option<bool>,
    selected: bool,
) -> egui::Response {
    // Reserve a shape slot for the highlight painted behind the row.
    let bg = ui.painter().add(egui::Shape::Noop);

    let frame = egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 3));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            caret(ui, open);
            let icon = display_icon(node, open);
            file_icon(ui, icon);
            let color = if node.is_dir {
                theme::LABEL_ON_DARK
            } else if selected {
                Color32::WHITE
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

    let response = ui.interact(inner.response.rect, inner.response.id, Sense::click());

    if selected || response.hovered() {
        let fill = if selected {
            theme::ACCENT
        } else {
            theme::ROW_HOVER
        };
        ui.painter().set(
            bg,
            egui::Shape::rect_filled(
                response.rect,
                CornerRadius::same(theme::CORNER_RADIUS),
                fill,
            ),
        );
    }
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }

    response
}

/// Paints the collapse caret in a fixed-width slot (blank for files).
fn caret(ui: &mut egui::Ui, open: Option<bool>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(CARET_WIDTH, ui.text_style_height(&egui::TextStyle::Body)),
        Sense::hover(),
    );
    if let Some(is_open) = open
        && ui.is_rect_visible(rect)
    {
        let glyph = if is_open {
            ph::CARET_DOWN
        } else {
            ph::CARET_RIGHT
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            glyph,
            FontId::proportional(CARET_SIZE),
            theme::MUTED_ON_DARK,
        );
    }
}

/// Chooses the icon: open/closed folder for directories, the file's own icon
/// otherwise.
fn display_icon(node: &TreeNode, open: Option<bool>) -> NodeIcon {
    match open {
        Some(true) => NodeIcon::FolderOpen,
        Some(false) => NodeIcon::FolderClosed,
        None => node.icon,
    }
}
