use eframe::egui::{self, Align2, CornerRadius, RichText};
use egui_phosphor::regular as ph;

use crate::presentation::components::atoms::file_icon::file_icon;
use crate::presentation::editor_controller::{EditorTab, TabId, reveal_label, save_shortcut_label};
use crate::presentation::theme;

const DOT_SLOT: f32 = 14.0;
const DOT_RADIUS: f32 = 4.0;
const CLOSE_SLOT: f32 = 16.0;
const DROP_INDICATOR_WIDTH: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabHit {
    Close,
    Save,
    Body,
}

struct HitZones {
    dot: egui::Rect,
    close: egui::Rect,
    unsaved: bool,
}

impl HitZones {
    fn hit(&self, pos: Option<egui::Pos2>) -> TabHit {
        let Some(pos) = pos else {
            return TabHit::Body;
        };
        if self.close.contains(pos) {
            TabHit::Close
        } else if self.unsaved && self.dot.contains(pos) {
            TabHit::Save
        } else {
            TabHit::Body
        }
    }
}

pub struct EditorTabRequest<'a> {
    pub tab: &'a EditorTab,
    pub index: usize,
    pub active: bool,
    pub any_unsaved: bool,
    pub scroll_to: bool,
}

#[derive(Default)]
pub struct EditorTabResponse {
    pub clicked: bool,
    pub close_clicked: bool,
    pub save_clicked: bool,
    pub save_all_clicked: bool,
    pub copy_path_clicked: bool,
    pub reveal_clicked: bool,
    pub drop_before: Option<usize>,
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

                let height = ui.text_style_height(&egui::TextStyle::Body);
                let (dot_rect, _) =
                    ui.allocate_exact_size(egui::vec2(DOT_SLOT, height), egui::Sense::hover());
                let (close_rect, _) =
                    ui.allocate_exact_size(egui::vec2(CLOSE_SLOT, height), egui::Sense::hover());
                (dot_rect, close_rect)
            })
            .inner
        });
    let (dot_rect, close_rect) = inner.inner;

    let body = ui.interact(
        inner.response.rect,
        inner.response.id,
        egui::Sense::click_and_drag(),
    );

    let zones = HitZones {
        dot: dot_rect,
        close: close_rect,
        unsaved,
    };
    let hover = zones.hit(body.hover_pos());
    let hover_close = hover == TabHit::Close;
    let hover_dot = hover == TabHit::Save;

    if unsaved {
        ui.painter()
            .circle_filled(dot_rect.center(), DOT_RADIUS, theme::UNSAVED_DOT);
    }
    if hover_close {
        ui.painter().rect_filled(
            close_rect,
            CornerRadius::same(theme::CORNER_RADIUS),
            theme::ROW_HOVER,
        );
    }
    ui.painter().text(
        close_rect.center(),
        Align2::CENTER_CENTER,
        ph::X,
        egui::FontId::proportional(theme::FONT_BODY),
        if hover_close {
            egui::Color32::WHITE
        } else {
            theme::MUTED_ON_DARK
        },
    );

    if body.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }

    match zones.hit(body.interact_pointer_pos()) {
        _ if !body.clicked() => {}
        TabHit::Close => response.close_clicked = true,
        TabHit::Save => response.save_clicked = true,
        TabHit::Body => response.clicked = true,
    }

    if body.drag_started() && zones.hit(body.interact_pointer_pos()) == TabHit::Body {
        egui::DragAndDrop::set_payload(ui.ctx(), tab.id);
    }

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

    if hover_close {
        body.clone().on_hover_text("Close");
    } else if hover_dot {
        body.clone()
            .on_hover_text(format!("Save ({})", save_shortcut_label()));
    }

    if scroll_to {
        body.scroll_to_me(Some(egui::Align::Center));
    }

    response
}

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

fn menu_item(ui: &mut egui::Ui, enabled: bool, glyph: &str, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(format!("{glyph}   {label}")).frame(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Rect, pos2};

    fn zones(unsaved: bool) -> HitZones {
        HitZones {
            dot: Rect::from_min_max(pos2(60.0, 0.0), pos2(74.0, 20.0)),
            close: Rect::from_min_max(pos2(74.0, 0.0), pos2(90.0, 20.0)),
            unsaved,
        }
    }

    #[test]
    fn the_close_slot_closes() {
        assert_eq!(zones(true).hit(Some(pos2(82.0, 10.0))), TabHit::Close);
        assert_eq!(zones(false).hit(Some(pos2(82.0, 10.0))), TabHit::Close);
    }

    #[test]
    fn the_dot_slot_saves_only_while_dirty() {
        assert_eq!(zones(true).hit(Some(pos2(67.0, 10.0))), TabHit::Save);
        assert_eq!(zones(false).hit(Some(pos2(67.0, 10.0))), TabHit::Body);
    }

    #[test]
    fn the_rest_of_the_tab_activates_it() {
        assert_eq!(zones(true).hit(Some(pos2(20.0, 10.0))), TabHit::Body);
        assert_eq!(zones(true).hit(Some(pos2(59.9, 10.0))), TabHit::Body);
    }

    #[test]
    fn no_pointer_is_inert() {
        assert_eq!(zones(true).hit(None), TabHit::Body);
    }

    #[test]
    fn the_slots_do_not_overlap() {
        assert!(!zones(true).dot.intersects(zones(true).close.shrink(0.01)));
    }
}
