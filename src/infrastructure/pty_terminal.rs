use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use crate::application::ports::{TerminalError, TerminalEvent, TerminalPort, TerminalSession};
use crate::domain::terminal::{
    CellStyle, Rgb, SCROLLBACK_LINES, ShellChoice, TerminalCell, TerminalPalette, TerminalSize,
    TerminalSnapshot, indexed_color,
};

pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct PtyTerminal {
    wake: Wake,
}

impl PtyTerminal {
    pub fn new(wake: Wake) -> Self {
        Self { wake }
    }
}

impl TerminalPort for PtyTerminal {
    fn open(
        &self,
        shell: &ShellChoice,
        cwd: &Path,
        size: TerminalSize,
        palette: TerminalPalette,
    ) -> Result<Box<dyn TerminalSession>, TerminalError> {
        let options = Options {
            shell: Some(Shell::new(shell.program.clone(), shell.args.clone())),
            working_directory: Some(cwd.to_path_buf()),
            drain_on_exit: true,
            ..Default::default()
        };

        let window_size = window_size(size);
        let pty = tty::new(&options, window_size, 0)
            .map_err(|e| TerminalError(format!("could not start {}: {e}", shell.program)))?;

        let (tx, events) = channel();
        let proxy = Proxy {
            tx,
            wake: self.wake.clone(),
        };

        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Default::default()
        };
        let term = Term::new(config, &SizeInfo::from(size), proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        let event_loop = EventLoop::new(term.clone(), proxy, pty, options.drain_on_exit, false)
            .map_err(|e| TerminalError(format!("could not start the PTY reader: {e}")))?;
        let notifier = Notifier(event_loop.channel());
        event_loop.spawn();

        Ok(Box::new(PtySession {
            term,
            notifier,
            events,
            palette,
        }))
    }
}

struct PtySession {
    term: Arc<FairMutex<Term<Proxy>>>,
    notifier: Notifier,
    events: Receiver<TerminalEvent>,
    palette: TerminalPalette,
}

impl TerminalSession for PtySession {
    fn write(&self, bytes: &[u8]) {
        let _ = self.notifier.0.send(Msg::Input(bytes.to_vec().into()));
    }

    fn resize(&self, size: TerminalSize) {
        let _ = self.notifier.0.send(Msg::Resize(window_size(size)));
        self.term.lock().resize(SizeInfo::from(size));
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_of(&term, &self.palette)
    }

    fn logical_text(&self) -> String {
        let term = self.term.lock();
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        term.bounds_to_string(start, end)
    }

    fn drain_events(&self) -> Vec<TerminalEvent> {
        self.events.try_iter().collect()
    }

    fn scroll(&self, lines: i32) {
        use alacritty_terminal::grid::Scroll;
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    fn clear(&self) {
        use alacritty_terminal::vte::ansi::{ClearMode, Handler};
        let mut term = self.term.lock();
        term.clear_screen(ClearMode::All);
        term.clear_screen(ClearMode::Saved);
    }

    fn select(&self, range: Option<((u16, u16), (u16, u16))>) {
        let mut term = self.term.lock();
        match range {
            None => term.selection = None,
            Some((from, to)) => {
                let offset = term.grid().display_offset();
                let anchor = grid_point(from, offset, &*term);
                let head = grid_point(to, offset, &*term);
                let mut selection = Selection::new(SelectionType::Simple, anchor, Side::Left);
                selection.update(head, Side::Right);
                term.selection = Some(selection);
            }
        }
    }

    fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    fn kill(&self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

#[derive(Clone)]
struct Proxy {
    tx: Sender<TerminalEvent>,
    wake: Wake,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let translated = match event {
            Event::Wakeup => Some(TerminalEvent::Wakeup),
            Event::Title(title) => Some(TerminalEvent::Title(title)),
            Event::ChildExit(status) => Some(TerminalEvent::ChildExit(status.code())),
            Event::ClipboardStore(_, text) => Some(TerminalEvent::ClipboardStore(text)),
            Event::ClipboardLoad(..) => Some(TerminalEvent::ClipboardLoad),
            Event::Bell => Some(TerminalEvent::Bell),
            _ => None,
        };

        if let Some(event) = translated {
            if self.tx.send(event).is_ok() {
                (self.wake)();
            }
        }
    }
}

fn snapshot_of(term: &Term<Proxy>, palette: &TerminalPalette) -> TerminalSnapshot {
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

fn grid_point(cell: (u16, u16), display_offset: usize, term: &Term<Proxy>) -> Point {
    let (col, row) = cell;
    let line = row as i32 - display_offset as i32;
    let columns = term.columns().saturating_sub(1);
    Point::new(Line(line), Column((col as usize).min(columns)))
}

fn window_size(size: TerminalSize) -> WindowSize {
    WindowSize {
        num_lines: size.rows,
        num_cols: size.cols,
        cell_width: size.cell_width.max(1),
        cell_height: size.cell_height.max(1),
    }
}

struct SizeInfo {
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    fn test_palette() -> TerminalPalette {
        TerminalPalette {
            named: [Rgb::new(0, 0, 0); 16],
            foreground: Rgb::new(0xcc, 0xcc, 0xcc),
            background: Rgb::new(0x11, 0x11, 0x18),
            cursor: Rgb::new(0xcc, 0xcc, 0xcc),
        }
    }

    fn size() -> TerminalSize {
        TerminalSize {
            cols: 40,
            rows: 8,
            cell_width: 8,
            cell_height: 16,
        }
    }

    fn screen_text(session: &dyn TerminalSession) -> String {
        let snapshot = session.snapshot();
        (0..snapshot.rows)
            .map(|row| {
                snapshot
                    .row(row)
                    .iter()
                    .map(|cell| cell.c)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn wait_for(
        session: &dyn TerminalSession,
        events: &mut Vec<TerminalEvent>,
        mut done: impl FnMut(&dyn TerminalSession, &[TerminalEvent]) -> bool,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            events.extend(session.drain_events());
            if done(session, events) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    #[cfg(unix)]
    fn a_session_runs_a_command_and_reports_its_exit_status() {
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = woken.clone();
        let terminal = PtyTerminal::new(Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));

        let shell = ShellChoice::new("/bin/echo", &["hello from the pty"]);
        let session = terminal
            .open(&shell, Path::new("/"), size(), test_palette())
            .expect("PTY should open");

        let mut events = Vec::new();
        let finished = wait_for(session.as_ref(), &mut events, |session, events| {
            screen_text(session).contains("hello from the pty")
                && events
                    .iter()
                    .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
        });

        assert!(
            finished,
            "expected the output and an exit event; grid was {:?}, events were {events:?}",
            screen_text(session.as_ref())
        );
        assert!(
            events.contains(&TerminalEvent::ChildExit(Some(0))),
            "echo should exit 0, got {events:?}"
        );
        assert!(
            woken.load(Ordering::Relaxed) > 0,
            "the UI must be woken when output arrives, or the panel never repaints"
        );
    }

    #[test]
    #[cfg(unix)]
    fn output_survives_the_child_that_produced_it() {
        let terminal = PtyTerminal::new(Arc::new(|| {}));
        let shell = ShellChoice::new(
            "/bin/sh",
            &["-c", "echo error: undefined reference; exit 2"],
        );
        let session = terminal
            .open(&shell, Path::new("/"), size(), test_palette())
            .expect("PTY should open");

        let mut events = Vec::new();
        let finished = wait_for(session.as_ref(), &mut events, |session, events| {
            events
                .iter()
                .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
                && screen_text(session).contains("undefined reference")
        });

        assert!(
            finished,
            "the error line must outlive the process; grid was {:?}",
            screen_text(session.as_ref())
        );

        assert!(screen_text(session.as_ref()).contains("error: undefined reference"));
        assert!(events.contains(&TerminalEvent::ChildExit(Some(2))));
    }

    #[test]
    #[cfg(unix)]
    fn scrolling_back_shows_history_instead_of_a_blank_gap() {
        let terminal = PtyTerminal::new(Arc::new(|| {}));
        let shell = ShellChoice::new(
            "/bin/sh",
            &[
                "-c",
                "i=1; while [ $i -le 30 ]; do echo \"line $i\"; i=$((i+1)); done",
            ],
        );
        let session = terminal
            .open(&shell, Path::new("/"), size(), test_palette())
            .expect("PTY should open");

        let mut events = Vec::new();
        let finished = wait_for(session.as_ref(), &mut events, |session, events| {
            events
                .iter()
                .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
                && screen_text(session).contains("line 30")
        });
        assert!(
            finished,
            "the tail of the output should be on screen; grid was {:?}",
            screen_text(session.as_ref())
        );

        session.scroll(10);

        let scrolled = screen_text(session.as_ref());
        assert!(
            !scrolled.contains("line 30"),
            "scrolling back must move the tail off screen; grid was {scrolled:?}"
        );
        assert!(
            scrolled.contains("line 15"),
            "scrolling back must bring history into view; grid was {scrolled:?}"
        );
        assert!(
            !scrolled.lines().last().unwrap_or_default().is_empty(),
            "the bottom rows must hold scrolled content, not a blank gap; grid was {scrolled:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_line_wider_than_the_grid_is_rejoined_for_parsing() {
        use crate::domain::diagnostics::{Severity, parse_diagnostics};

        let terminal = PtyTerminal::new(Arc::new(|| {}));
        let long_path = "src/very/deeply/nested/directory/effect_processor.cpp";
        let message = "use of undeclared identifier 'cutof'; did you mean 'cutoff'?";
        let line = format!("{long_path}:14:24: error: {message}");
        let shell = ShellChoice::new("/bin/sh", &["-c", &format!("printf '%s\\n' \"{line}\"")]);

        let session = terminal
            .open(&shell, Path::new("/"), size(), test_palette())
            .expect("PTY should open");

        let mut events = Vec::new();
        let finished = wait_for(session.as_ref(), &mut events, |session, events| {
            events
                .iter()
                .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
                && !parse_diagnostics(&session.logical_text()).is_empty()
        });
        assert!(
            finished,
            "nothing parsed; logical text was {:?}",
            session.logical_text()
        );

        let rendered = screen_text(session.as_ref());
        assert!(
            !rendered.lines().any(|row| row.contains(&line)),
            "expected the grid to have wrapped the line, but a row held it whole"
        );

        let parsed = parse_diagnostics(&session.logical_text());
        assert_eq!(parsed.len(), 1, "got {parsed:?}");
        assert_eq!(parsed[0].severity, Severity::Error);
        assert_eq!(parsed[0].file.as_deref(), Some(long_path));
        assert_eq!(parsed[0].line, Some(14));
        assert_eq!(parsed[0].column, Some(24));
        assert_eq!(parsed[0].message, message);
    }

    #[test]
    #[cfg(unix)]
    fn a_failing_command_reports_its_non_zero_status() {
        let terminal = PtyTerminal::new(Arc::new(|| {}));
        let shell = ShellChoice::new("/bin/sh", &["-c", "exit 3"]);
        let session = terminal
            .open(&shell, Path::new("/"), size(), test_palette())
            .expect("PTY should open");

        let mut events = Vec::new();
        let finished = wait_for(session.as_ref(), &mut events, |_, events| {
            events
                .iter()
                .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
        });

        assert!(finished, "no exit event arrived: {events:?}");
        assert!(
            events.contains(&TerminalEvent::ChildExit(Some(3))),
            "the build's real exit code is what drives the status, got {events:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn the_real_default_shell_opens_accepts_input_and_reports_its_exit_code() {
        use crate::domain::terminal::{Platform, shell_for};

        let shell = shell_for(
            Platform::current(),
            &|key| std::env::var(key).ok(),
            &|path| path.exists(),
        );
        let terminal = PtyTerminal::new(Arc::new(|| {}));
        let session = terminal
            .open(&shell, Path::new("/"), size(), test_palette())
            .unwrap_or_else(|e| panic!("{} should open: {e}", shell.program));

        session.write(b"exit 7\n");

        let mut events = Vec::new();
        let finished = wait_for(session.as_ref(), &mut events, |_, events| {
            events
                .iter()
                .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
        });

        assert!(finished, "{} never exited: {events:?}", shell.program);
        assert!(
            events.contains(&TerminalEvent::ChildExit(Some(7))),
            "input must reach the shell and its status come back, got {events:?}"
        );
    }

    #[test]
    fn opening_a_missing_program_fails_with_its_name() {
        let terminal = PtyTerminal::new(Arc::new(|| {}));
        let shell = ShellChoice::new("/nonexistent/definitely-not-a-shell", &[]);
        let opened = terminal.open(&shell, Path::new("/"), size(), test_palette());

        if let Err(error) = opened {
            assert!(
                error.0.contains("definitely-not-a-shell"),
                "the error should name the program: {error}"
            );
        }
    }

    #[test]
    fn the_xterm_cube_and_greyscale_ramp_resolve_to_known_values() {
        let palette = test_palette();
        // 16 is the cube's origin: pure black.
        assert_eq!(indexed_color(16, &palette), Rgb::new(0, 0, 0));
        // 231 is its opposite corner: pure white.
        assert_eq!(indexed_color(231, &palette), Rgb::new(255, 255, 255));
        // 196 is the classic bright red of the cube.
        assert_eq!(indexed_color(196, &palette), Rgb::new(255, 0, 0));
        // The greyscale ramp starts at 8 and steps by 10.
        assert_eq!(indexed_color(232, &palette), Rgb::new(8, 8, 8));
        assert_eq!(indexed_color(255, &palette), Rgb::new(238, 238, 238));
    }
}
