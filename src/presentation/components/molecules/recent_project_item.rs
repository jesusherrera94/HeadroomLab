use eframe::egui::{self, CornerRadius, RichText, Stroke};

use crate::domain::project::RecentProject;
use crate::presentation::theme;

pub fn recent_project_item(ui: &mut egui::Ui, project: &RecentProject) -> bool {
    let bg = ui.painter().add(egui::Shape::Noop);

    let frame = egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 7));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            badge(ui, &project.abbr());
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&project.name)
                        .font(theme::body_font())
                        .color(theme::LABEL_ON_DARK),
                );
                ui.label(
                    RichText::new(ellipsize(&project.path.to_string_lossy(), 52))
                        .font(egui::FontId::monospace(theme::FONT_SMALL))
                        .color(theme::MUTED_ON_DARK),
                );
            });
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

fn badge(ui: &mut egui::Ui, abbr: &str) {
    let size = 26.0;
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(5),
        theme::INSET_SURFACE,
        Stroke::new(1.0, theme::INSET_BORDER),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        abbr,
        egui::FontId::monospace(theme::FONT_SMALL),
        theme::LOGO_STROKE,
    );
}

fn ellipsize(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
    format!("…{tail}")
}
