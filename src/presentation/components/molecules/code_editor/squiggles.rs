//! Wavy underlines beneath diagnostic ranges.

use std::ops::Range;

use eframe::egui::{self, text::CCursor};

pub(super) fn paint_squiggles(
    ui: &egui::Ui,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    squiggles: &[(Range<usize>, egui::Color32)],
) {
    const PERIOD: f32 = 4.0;
    const AMPLITUDE: f32 = 1.5;
    const OFFSET: f32 = 1.0;

    let painter = ui.painter();
    for (range, color) in squiggles {
        if range.start >= range.end {
            continue;
        }

        let start = galley.pos_from_cursor(CCursor::new(range.start));
        let end = galley.pos_from_cursor(CCursor::new(range.end));
        let right = if (end.top() - start.top()).abs() < 0.5 {
            end.left()
        } else {
            galley.rect.right()
        };

        let y = galley_pos.y + start.bottom() + OFFSET;
        let left = galley_pos.x + start.left();
        let right = galley_pos.x + right;
        if right <= left {
            continue;
        }

        let mut points = Vec::with_capacity(((right - left) / PERIOD).ceil() as usize + 2);
        let mut x = left;
        let mut up = true;
        while x < right {
            points.push(egui::pos2(x, if up { y } else { y + AMPLITUDE }));
            x += PERIOD / 2.0;
            up = !up;
        }
        points.push(egui::pos2(right, if up { y } else { y + AMPLITUDE }));

        painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, *color)));
    }
}
