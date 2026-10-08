//! The renderer-agnostic picture of a terminal: colours, cell styles and the
//! visible grid.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPalette {
    pub named: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}

pub fn indexed_color(index: u8, palette: &TerminalPalette) -> Rgb {
    match index {
        0..=15 => palette.named[index as usize],
        16..=231 => {
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let i = index as usize - 16;
            Rgb::new(LEVELS[i / 36], LEVELS[(i % 36) / 6], LEVELS[i % 6])
        }
        232..=255 => {
            let level = 8 + (index as u16 - 232) * 10;
            let level = level.min(255) as u8;
            Rgb::new(level, level, level)
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub style: CellStyle,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<TerminalCell>,
    pub cursor: Option<(u16, u16)>,
    pub display_offset: usize,
    pub history_len: usize,
}

impl TerminalSnapshot {
    pub fn row(&self, row: u16) -> &[TerminalCell] {
        let cols = self.cols as usize;
        let start = row as usize * cols;
        self.cells.get(start..start + cols).unwrap_or(&[])
    }
}
