//! `ClipboardPort` over the OS clipboard, via `arboard`.
//!
//! `arboard` is already in the dependency tree underneath `eframe` — this is the
//! same clipboard egui itself copies through, reached directly because egui
//! exposes no *read* side.
//!
//! On Linux this needs the `wayland-data-control` feature (declared in
//! `Cargo.toml`) to reach a native Wayland session; without it `arboard` is
//! X11-only and a Wayland user's reads would work solely through XWayland. The
//! feature relies on the `wlr-data-control` protocol, which not every compositor
//! implements — where it is missing `arboard` falls back to X11, and where that
//! fails too `read` returns `None` and the Paste menu item greys out. Keyboard
//! paste is unaffected either way: it never comes through here.

use std::cell::RefCell;

use arboard::Clipboard;

use crate::application::ports::ClipboardPort;

pub struct SystemClipboard {
    /// Held open rather than rebuilt per read: on X11 every `Clipboard` spawns a
    /// thread. `RefCell` because the port takes `&self` while `arboard` wants
    /// `&mut` — the UI is single-threaded, so there is never contention.
    ///
    /// `None` when the clipboard could not be opened at all, which is a normal
    /// state on a headless machine rather than an error worth reporting.
    inner: RefCell<Option<Clipboard>>,
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Clipboard::new().ok()),
        }
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardPort for SystemClipboard {
    fn read(&self) -> Option<String> {
        let mut clipboard = self.inner.borrow_mut();
        // An empty clipboard reads as nothing to paste, so Paste greys out
        // rather than offering to insert an empty string.
        clipboard
            .as_mut()?
            .get_text()
            .ok()
            .filter(|text| !text.is_empty())
    }
}
