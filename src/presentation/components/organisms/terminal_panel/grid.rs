//! Painting a terminal snapshot: background runs, styled text runs and the
//! cursor.

use eframe::egui::{self, text::LayoutJob};

use super::has_focus;
use crate::domain::terminal::{Rgb, TerminalCell, TerminalSnapshot};
use crate::presentation::theme;

pub(super) fn paint_grid(
    ui: &egui::Ui,
    snapshot: &TerminalSnapshot,
    origin: egui::Pos2,
    cell_width: f32,
    cell_height: f32,
) {
    let painter = ui.painter();
    let font = egui::FontId::monospace(theme::FONT_BODY);
    let default_bg = theme::PLOT_FRAME_BACKGROUND;

    for row in 0..snapshot.rows {
        let cells = snapshot.row(row);
        if cells.is_empty() {
            continue;
        }
        let y = origin.y + row as f32 * cell_height;

        let mut run_start = 0usize;
        for column in 0..=cells.len() {
            let ends = column == cells.len()
                || background_of(&cells[column]) != background_of(&cells[run_start]);
            if !ends {
                continue;
            }
            let color = background_of(&cells[run_start]);
            if color != default_bg {
                let x = origin.x + run_start as f32 * cell_width;
                let width = (column - run_start) as f32 * cell_width;
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, cell_height)),
                    0.0,
                    color,
                );
            }
            run_start = column;
        }

        let mut job = LayoutJob::default();
        let mut text = String::with_capacity(cells.len());
        let mut run_start = 0usize;
        for column in 0..=cells.len() {
            let ends = column == cells.len() || !same_run(&cells[column], &cells[run_start]);
            if !ends {
                continue;
            }

            let run: String = cells[run_start..column].iter().map(|c| c.c).collect();
            if !run.trim().is_empty() {
                let start = text.len();
                text.push_str(&run);
                job.sections.push(egui::text::LayoutSection {
                    leading_space: 0.0,
                    byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(text.len()),
                    format: format_of(&cells[run_start], &font),
                });
            } else {
                text.push_str(&run);
                let start = text.len() - run.len();
                job.sections.push(egui::text::LayoutSection {
                    leading_space: 0.0,
                    byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(text.len()),
                    format: egui::TextFormat {
                        font_id: font.clone(),
                        color: egui::Color32::TRANSPARENT,
                        ..Default::default()
                    },
                });
            }
            run_start = column;
        }

        if !text.trim().is_empty() {
            job.text = text;
            job.wrap.max_width = f32::INFINITY;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            painter.galley(egui::pos2(origin.x, y), galley, theme::LABEL_ON_DARK);
        }
    }

    if let Some((col, row)) = snapshot.cursor {
        let rect = egui::Rect::from_min_size(
            egui::pos2(
                origin.x + col as f32 * cell_width,
                origin.y + row as f32 * cell_height,
            ),
            egui::vec2(cell_width, cell_height),
        );
        if has_focus(ui.ctx()) {
            painter.rect_filled(rect, 0.0, theme::LABEL_ON_DARK.gamma_multiply(0.55));
        } else {
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, theme::MUTED_ON_DARK),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn background_of(cell: &TerminalCell) -> egui::Color32 {
    if cell.selected {
        theme::TERMINAL_SELECTION
    } else {
        theme::terminal_color(cell.bg)
    }
}

fn same_run(a: &TerminalCell, b: &TerminalCell) -> bool {
    a.fg == b.fg && a.style == b.style && a.selected == b.selected
}

fn format_of(cell: &TerminalCell, font: &egui::FontId) -> egui::TextFormat {
    egui::TextFormat {
        font_id: font.clone(),
        color: foreground_of(cell),
        underline: if cell.style.underline {
            egui::Stroke::new(1.0, foreground_of(cell))
        } else {
            egui::Stroke::NONE
        },
        strikethrough: if cell.style.strikeout {
            egui::Stroke::new(1.0, foreground_of(cell))
        } else {
            egui::Stroke::NONE
        },
        italics: cell.style.italic,
        ..Default::default()
    }
}

fn foreground_of(cell: &TerminalCell) -> egui::Color32 {
    let color = theme::terminal_color(cell.fg);
    if cell.style.bold {
        brighten(cell.fg)
    } else {
        color
    }
}

fn brighten(rgb: Rgb) -> egui::Color32 {
    let lift = |c: u8| ((c as u16 * 5 / 4).min(255)) as u8;
    egui::Color32::from_rgb(lift(rgb.r), lift(rgb.g), lift(rgb.b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::terminal::CellStyle;

    #[test]
    fn a_run_breaks_on_colour_style_or_selection() {
        let base = TerminalCell {
            c: 'a',
            fg: Rgb::new(1, 2, 3),
            bg: Rgb::new(0, 0, 0),
            style: CellStyle::default(),
            selected: false,
        };

        let mut other = base;
        other.c = 'b';
        assert!(same_run(&base, &other));

        let mut recoloured = base;
        recoloured.fg = Rgb::new(9, 9, 9);
        assert!(!same_run(&base, &recoloured));

        let mut bolded = base;
        bolded.style.bold = true;
        assert!(!same_run(&base, &bolded));

        let mut selected = base;
        selected.selected = true;
        assert!(!same_run(&base, &selected));
    }

    #[test]
    fn bold_text_is_brightened_without_overflowing_a_channel() {
        assert_eq!(
            brighten(Rgb::new(100, 220, 0)),
            egui::Color32::from_rgb(125, 255, 0)
        );
    }

    #[test]
    fn selection_overrides_the_cells_own_background() {
        let mut cell = TerminalCell {
            c: ' ',
            fg: Rgb::new(0, 0, 0),
            bg: Rgb::new(0x40, 0, 0),
            style: CellStyle::default(),
            selected: false,
        };
        assert_eq!(background_of(&cell), theme::terminal_color(cell.bg));
        cell.selected = true;
        assert_eq!(background_of(&cell), theme::TERMINAL_SELECTION);
    }
}
