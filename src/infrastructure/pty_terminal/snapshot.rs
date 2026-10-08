//! Copying alacritty's grid into a renderer-agnostic `TerminalSnapshot`,
//! resolving every colour against the app's palette.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use super::Proxy;
use crate::domain::terminal::{
    CellStyle, Rgb, TerminalCell, TerminalPalette, TerminalSnapshot, indexed_color,
};

pub(super) fn snapshot_of(term: &Term<Proxy>, palette: &TerminalPalette) -> TerminalSnapshot {
    let cols = term.columns() as u16;
    let rows = term.screen_lines() as u16;
    let content = term.renderable_content();

    let default = TerminalCell {
        c: ' ',
        fg: palette.foreground,
        bg: palette.background,
        style: CellStyle::default(),
        selected: false,
    };
    let mut cells = vec![default; cols as usize * rows as usize];

    let selection = content.selection;
    let display_offset = content.display_offset as i32;
    for item in content.display_iter {
        let line = item.point.line.0 + display_offset;
        let column = item.point.column.0;
        if line < 0 || line as u16 >= rows || column as u16 >= cols {
            continue;
        }

        let flags = item.cell.flags;
        let c = if flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            item.cell.c
        };

        let mut fg = resolve(item.cell.fg, palette, content.colors);
        let mut bg = resolve(item.cell.bg, palette, content.colors);
        if flags.contains(Flags::DIM) && !flags.contains(Flags::BOLD) {
            fg = dimmed(fg);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }

        let selected = selection.is_some_and(|range| range.contains(item.point));

        cells[line as usize * cols as usize + column as usize] = TerminalCell {
            c,
            fg,
            bg,
            style: CellStyle {
                bold: flags.contains(Flags::BOLD),
                dim: flags.contains(Flags::DIM),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                strikeout: flags.contains(Flags::STRIKEOUT),
            },
            selected,
        };
    }

    let cursor_point = content.cursor.point;
    let cursor_line = cursor_point.line.0 + display_offset;
    let cursor = (content.cursor.shape != alacritty_terminal::vte::ansi::CursorShape::Hidden
        && cursor_line >= 0
        && (cursor_line as u16) < rows
        && (cursor_point.column.0 as u16) < cols)
        .then(|| (cursor_point.column.0 as u16, cursor_line as u16));

    TerminalSnapshot {
        cols,
        rows,
        cells,
        cursor,
        display_offset: content.display_offset,
        history_len: term.grid().history_size(),
    }
}

fn resolve(
    color: Color,
    palette: &TerminalPalette,
    colors: &alacritty_terminal::term::color::Colors,
) -> Rgb {
    match color {
        Color::Spec(rgb) => Rgb::new(rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => match colors[index as usize] {
            Some(rgb) => Rgb::new(rgb.r, rgb.g, rgb.b),
            None => indexed_color(index, palette),
        },
        Color::Named(named) => match colors[named as usize] {
            Some(rgb) => Rgb::new(rgb.r, rgb.g, rgb.b),
            None => named_color(named, palette),
        },
    }
}

fn named_color(named: NamedColor, palette: &TerminalPalette) -> Rgb {
    use NamedColor as N;
    match named {
        N::Black => palette.named[0],
        N::Red => palette.named[1],
        N::Green => palette.named[2],
        N::Yellow => palette.named[3],
        N::Blue => palette.named[4],
        N::Magenta => palette.named[5],
        N::Cyan => palette.named[6],
        N::White => palette.named[7],
        N::BrightBlack => palette.named[8],
        N::BrightRed => palette.named[9],
        N::BrightGreen => palette.named[10],
        N::BrightYellow => palette.named[11],
        N::BrightBlue => palette.named[12],
        N::BrightMagenta => palette.named[13],
        N::BrightCyan => palette.named[14],
        N::BrightWhite => palette.named[15],

        N::Foreground | N::BrightForeground => palette.foreground,
        N::Background => palette.background,
        N::Cursor => palette.cursor,

        N::DimBlack => dimmed(palette.named[0]),
        N::DimRed => dimmed(palette.named[1]),
        N::DimGreen => dimmed(palette.named[2]),
        N::DimYellow => dimmed(palette.named[3]),
        N::DimBlue => dimmed(palette.named[4]),
        N::DimMagenta => dimmed(palette.named[5]),
        N::DimCyan => dimmed(palette.named[6]),
        N::DimWhite => dimmed(palette.named[7]),
        N::DimForeground => dimmed(palette.foreground),
    }
}

fn dimmed(rgb: Rgb) -> Rgb {
    Rgb::new(
        (rgb.r as u16 * 2 / 3) as u8,
        (rgb.g as u16 * 2 / 3) as u8,
        (rgb.b as u16 * 2 / 3) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> TerminalPalette {
        let mut named = [Rgb::new(0, 0, 0); 16];
        for (index, color) in named.iter_mut().enumerate() {
            *color = Rgb::new(index as u8 * 10, 0, 0);
        }
        TerminalPalette {
            named,
            foreground: Rgb::new(0xcc, 0xcc, 0xcc),
            background: Rgb::new(0x11, 0x11, 0x18),
            cursor: Rgb::new(0xff, 0x00, 0xff),
        }
    }

    #[test]
    fn dimming_keeps_two_thirds_of_each_channel() {
        assert_eq!(dimmed(Rgb::new(255, 150, 0)), Rgb::new(170, 100, 0));
    }

    #[test]
    fn named_colours_come_from_the_palette_and_dim_variants_are_dimmed() {
        let palette = palette();
        assert_eq!(named_color(NamedColor::Red, &palette), palette.named[1]);
        assert_eq!(
            named_color(NamedColor::BrightWhite, &palette),
            palette.named[15]
        );
        assert_eq!(
            named_color(NamedColor::Foreground, &palette),
            palette.foreground
        );
        assert_eq!(
            named_color(NamedColor::Background, &palette),
            palette.background
        );
        assert_eq!(named_color(NamedColor::Cursor, &palette), palette.cursor);
        assert_eq!(
            named_color(NamedColor::DimRed, &palette),
            dimmed(palette.named[1])
        );
        assert_eq!(
            named_color(NamedColor::DimForeground, &palette),
            dimmed(palette.foreground)
        );
    }
}
