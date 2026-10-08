//! Translating the app's terminal size and cell coordinates into alacritty's.

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;

use super::Proxy;
use crate::domain::terminal::{SCROLLBACK_LINES, TerminalSize};

pub(super) fn grid_point(cell: (u16, u16), display_offset: usize, term: &Term<Proxy>) -> Point {
    let (col, row) = cell;
    let line = row as i32 - display_offset as i32;
    let columns = term.columns().saturating_sub(1);
    Point::new(Line(line), Column((col as usize).min(columns)))
}

pub(super) fn window_size(size: TerminalSize) -> WindowSize {
    WindowSize {
        num_lines: size.rows,
        num_cols: size.cols,
        cell_width: size.cell_width.max(1),
        cell_height: size.cell_height.max(1),
    }
}

pub(super) struct SizeInfo {
    cols: usize,
    rows: usize,
}

impl From<TerminalSize> for SizeInfo {
    fn from(size: TerminalSize) -> Self {
        Self {
            cols: size.cols as usize,
            rows: size.rows as usize,
        }
    }
}

impl Dimensions for SizeInfo {
    fn total_lines(&self) -> usize {
        self.rows + SCROLLBACK_LINES
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(cell_width: u16, cell_height: u16) -> TerminalSize {
        TerminalSize {
            cols: 80,
            rows: 24,
            cell_width,
            cell_height,
        }
    }

    #[test]
    fn a_zero_cell_size_is_clamped_before_it_reaches_the_pty() {
        // The panel is measured before the font is, so a 0×0 cell can arrive.
        let window = window_size(size(0, 0));
        assert_eq!(
            (
                window.num_cols,
                window.num_lines,
                window.cell_width,
                window.cell_height
            ),
            (80, 24, 1, 1)
        );
    }

    #[test]
    fn the_grid_reserves_the_scrollback_beyond_the_screen() {
        let info = SizeInfo::from(size(8, 16));
        assert_eq!(info.columns(), 80);
        assert_eq!(info.screen_lines(), 24);
        assert_eq!(info.total_lines(), 24 + SCROLLBACK_LINES);
    }
}
