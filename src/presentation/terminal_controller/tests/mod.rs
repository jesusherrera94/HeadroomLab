//! Shared fakes for the terminal controller's tests.

mod builds;
mod sessions;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::*;
use crate::application::ports::{
    ClipboardPort, TerminalError, TerminalEvent, TerminalPort, TerminalSession,
};
use crate::domain::diagnostics::Counts;
use crate::domain::terminal::{
    BuildKind, BuildStatus, Rgb, ShellChoice, TerminalCell, TerminalPalette, TerminalSize,
    TerminalSnapshot,
};

fn palette() -> TerminalPalette {
    TerminalPalette {
        named: [Rgb::new(0, 0, 0); 16],
        foreground: Rgb::new(0xcc, 0xcc, 0xcc),
        background: Rgb::new(0, 0, 0),
        cursor: Rgb::new(0xcc, 0xcc, 0xcc),
    }
}

#[derive(Default)]
struct FakeInner {
    written: RefCell<Vec<u8>>,
    killed: RefCell<bool>,
    cleared: RefCell<bool>,
    resized: RefCell<Vec<TerminalSize>>,
    events: RefCell<Vec<TerminalEvent>>,
    output: RefCell<String>,
}

impl TerminalSession for Rc<FakeInner> {
    fn write(&self, bytes: &[u8]) {
        self.written.borrow_mut().extend_from_slice(bytes);
    }
    fn resize(&self, size: TerminalSize) {
        self.resized.borrow_mut().push(size);
    }
    fn snapshot(&self) -> TerminalSnapshot {
        TerminalSnapshot {
            cols: 1,
            rows: 1,
            cells: vec![TerminalCell {
                c: ' ',
                fg: Rgb::new(0, 0, 0),
                bg: Rgb::new(0, 0, 0),
                style: Default::default(),
                selected: false,
            }],
            cursor: None,
            display_offset: 0,
            history_len: 0,
        }
    }
    fn logical_text(&self) -> String {
        self.output.borrow().clone()
    }
    fn drain_events(&self) -> Vec<TerminalEvent> {
        std::mem::take(&mut *self.events.borrow_mut())
    }
    fn scroll(&self, _: i32) {}
    fn clear(&self) {
        *self.cleared.borrow_mut() = true;
    }
    fn select(&self, _: Option<((u16, u16), (u16, u16))>) {}
    fn selection_text(&self) -> Option<String> {
        None
    }
    fn kill(&self) {
        *self.killed.borrow_mut() = true;
    }
}

#[derive(Default)]
struct FakePort {
    opened: RefCell<Vec<Rc<FakeInner>>>,
    fail_with: RefCell<Option<String>>,
}

impl TerminalPort for FakePort {
    fn open(
        &self,
        _shell: &ShellChoice,
        _cwd: &Path,
        _size: TerminalSize,
        _palette: TerminalPalette,
    ) -> Result<Box<dyn TerminalSession>, TerminalError> {
        if let Some(message) = self.fail_with.borrow().clone() {
            return Err(TerminalError(message));
        }
        let inner = Rc::new(FakeInner::default());
        self.opened.borrow_mut().push(inner.clone());
        Ok(Box::new(inner))
    }
}

struct FakeClipboard(Option<String>);
impl ClipboardPort for FakeClipboard {
    fn read(&self) -> Option<String> {
        self.0.clone()
    }
}

fn state_over(port: Rc<FakePort>, clipboard: Option<&str>) -> TerminalState {
    TerminalState::new(
        port,
        Rc::new(FakeClipboard(clipboard.map(|s| s.to_string()))),
        palette(),
        Path::new("/proj"),
    )
}
