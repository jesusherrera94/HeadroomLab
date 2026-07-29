//! One tab in the editor's tab strip: kind icon, file name, an unsaved ● dot
//! that saves when pressed, and a close (×) button. The active tab is filled to
//! stand out. Tabs are also drag sources and drop targets for reordering, and
//! carry a context menu. Returns what the user did; the caller applies it.
//!
//! Two subtleties worth knowing before editing this file:
//!
//! * The tab body is interacted with *after* the inner ● and × widgets and
//!   overlaps them, so every body action is guarded against the inner ones
//!   having fired. Add a new inner button → add it to `button_rects` and to the
//!   `clicked()` guard, or it will double-fire with "activate this tab".
//! * `Response::dnd_release_payload` *takes* the drag payload, so the landing
//!   slot must be computed from `dnd_hover_payload` first. Reversing the two
//!   loses the drop.

use eframe::egui::{self, CornerRadius, RichText};
use egui_phosphor::regular as ph;

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::editor_controller::{EditorTab, TabId, reveal_label, save_shortcut_label};
use crate::presentation::theme;

/// Width reserved for the ● whether or not it is drawn, so saving a file doesn't
/// shuffle the rest of the strip sideways.
const DOT_SLOT: f32 = 14.0;
const DOT_RADIUS: f32 = 4.0;
/// Thickness of the "it lands here" line painted during a drag.
const DROP_INDICATOR_WIDTH: f32 = 2.0;

pub struct EditorTabRequest<'a> {
    pub tab: &'a EditorTab,
    /// This tab's position in the strip, used to name drop slots.
    pub index: usize,
    pub active: bool,
    /// Whether *any* open buffer is dirty — enables "Save All" in the menu.
    pub any_unsaved: bool,
    /// Scroll this tab into view this frame (the active tab changed from
    /// outside the strip, or a reorder just moved it).
    pub scroll_to: bool,
}

#[derive(Default)]
pub struct EditorTabResponse {
    /// The tab body was clicked (activate it).
    pub clicked: bool,
    /// The close (×) button, or the menu's Close, was clicked.
    pub close_clicked: bool,
    /// The ● , or the menu's Save, was clicked.
    pub save_clicked: bool,
    pub save_all_clicked: bool,
    pub copy_path_clicked: bool,
    pub reveal_clicked: bool,
    /// Where a tab currently being dragged over this one would land, in the
    /// pre-move index space this strip was painted in.
    pub drop_before: Option<usize>,
    /// A drag was released over this tab, carrying the dragged tab's identity.
    /// Always accompanied by `drop_before`.
    pub dropped: Option<TabId>,
}

pub fn editor_tab(ui: &mut egui::Ui, request: EditorTabRequest<'_>) -> EditorTabResponse {
    let EditorTabRequest {
        tab,
        index,
        active,
        any_unsaved,
        scroll_to,
    } = request;

    let mut response = EditorTabResponse::default();
    let unsaved = tab.unsaved();

    let fill = if active {
        theme::TAB_ACTIVE_BACKGROUND
    } else {
        egui::Color32::TRANSPARENT
    };

    // Rects of the inner buttons: a press that lands on one must never start a
    // tab drag, or the ● and × become unreliable.
    let mut button_rects: Vec<egui::Rect> = Vec::new();

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

                // The ● slot is always allocated; only the dot is conditional.
                let row_height = ui.text_style_height(&egui::TextStyle::Body);
                let sense = if unsaved {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                };
                let (dot_rect, dot) =
                    ui.allocate_exact_size(egui::vec2(DOT_SLOT, row_height), sense);
                if unsaved {
                    ui.painter()
                        .circle_filled(dot_rect.center(), DOT_RADIUS, theme::UNSAVED_DOT);
                    button_rects.push(dot_rect);
                    if dot.hovered() {
                        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                    }
                    if dot
                        .on_hover_text(format!("Save ({})", save_shortcut_label()))
                        .clicked()
                    {
                        response.save_clicked = true;
                    }
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
                button_rects.push(close.rect);
                if close.hovered() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                }
                if close.on_hover_text("Close").clicked() {
                    response.close_clicked = true;
                }
            });
        });

    let body = ui.interact(
        inner.response.rect,
        inner.response.id,
        egui::Sense::click_and_drag(),
    );
    if body.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    // A click anywhere but the ● or the × activates the tab.
    if body.clicked() && !response.close_clicked && !response.save_clicked {
        response.clicked = true;
    }

    // -- Drag source ------------------------------------------------------
    // Set by hand rather than via `dnd_set_drag_payload` so a press that began
    // on the ● or the × can be excluded.
    if body.drag_started()
        && !body
            .interact_pointer_pos()
            .is_some_and(|pos| button_rects.iter().any(|r| r.contains(pos)))
    {
        egui::DragAndDrop::set_payload(ui.ctx(), tab.id);
    }

    // -- Drop target ------------------------------------------------------
    // Hover before release: `dnd_release_payload` takes the payload, and the
    // nesting guarantees a reported drop always has a landing slot with it.
    if let Some(dragged) = body.dnd_hover_payload::<TabId>()
        && *dragged != tab.id
    {
        let rect = body.rect;
        let before = ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| pos.x < rect.center().x);
        response.drop_before = Some(if before { index } else { index + 1 });

        let x = if before { rect.left() } else { rect.right() };
        ui.painter().vline(
            x,
            rect.y_range(),
            egui::Stroke::new(DROP_INDICATOR_WIDTH, theme::ACCENT),
        );

        if let Some(dropped) = body.dnd_release_payload::<TabId>() {
            response.dropped = Some(*dropped);
        }
    }

    body.context_menu(|ui| tab_menu(ui, tab, unsaved, any_unsaved, &mut response));

    if scroll_to {
        body.scroll_to_me(Some(egui::Align::Center));
    }

    response
}

/// Right-click menu for a tab. Save is disabled on a clean or non-editable
/// buffer, Save All when nothing anywhere is dirty — the menu should never offer
/// an action that would silently do nothing.
fn tab_menu(
    ui: &mut egui::Ui,
    tab: &EditorTab,
    unsaved: bool,
    any_unsaved: bool,
    response: &mut EditorTabResponse,
) {
    ui.set_min_width(210.0);
    let savable = unsaved && tab.content.is_editable();

    if menu_item(ui, savable, ph::FLOPPY_DISK, "Save").clicked() {
        response.save_clicked = true;
        ui.close();
    }
    if menu_item(ui, any_unsaved, ph::FLOPPY_DISK_BACK, "Save All").clicked() {
        response.save_all_clicked = true;
        ui.close();
    }
    ui.separator();
    if menu_item(ui, true, ph::X, "Close").clicked() {
        response.close_clicked = true;
        ui.close();
    }
    ui.separator();
    if menu_item(ui, true, ph::COPY, "Copy Path").clicked() {
        response.copy_path_clicked = true;
        ui.close();
    }
    if menu_item(ui, true, ph::ARROW_SQUARE_OUT, reveal_label()).clicked() {
        response.reveal_clicked = true;
        ui.close();
    }
}

/// Matches the explorer's menu rows (`file_explorer::menu_item`) so both context
/// menus read as the same control.
fn menu_item(ui: &mut egui::Ui, enabled: bool, glyph: &str, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(format!("{glyph}   {label}")).frame(false),
    )
}
