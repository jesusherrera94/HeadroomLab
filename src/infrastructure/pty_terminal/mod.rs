mod geometry;
mod snapshot;

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};

use crate::application::ports::{TerminalError, TerminalEvent, TerminalPort, TerminalSession};
use crate::domain::terminal::{
    SCROLLBACK_LINES, ShellChoice, TerminalPalette, TerminalSize, TerminalSnapshot,
};

use geometry::{SizeInfo, grid_point, window_size};
use snapshot::snapshot_of;

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

#[cfg(test)]
mod tests;
