use std::cell::RefCell;

use arboard::Clipboard;

use crate::application::ports::ClipboardPort;

pub struct SystemClipboard {
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
        clipboard
            .as_mut()?
            .get_text()
            .ok()
            .filter(|text| !text.is_empty())
    }
}
