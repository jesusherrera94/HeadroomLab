mod build;
mod events;
mod sessions;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::application::ports::{ClipboardPort, TerminalPort, TerminalSession};
use crate::domain::diagnostics::{Counts, Diagnostic, counts};
use crate::domain::terminal::{BuildKind, BuildStatus, ShellChoice, TerminalPalette, TerminalSize};

pub use build::run_build;
pub use events::tick;
pub use sessions::{
    activate, active_snapshot, clear_active, close, default_shell, ensure_open, open_shell,
    paste_active, resize, restart, write_active,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Shell,
    Build(BuildKind),
}

pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub kind: SessionKind,
    pub command: ShellChoice,
    pub inner: Option<Box<dyn TerminalSession>>,
    pub exited: Option<Option<i32>>,
    pub status: Option<BuildStatus>,
    pub error: Option<String>,
    pub drag_anchor: Option<(u16, u16)>,
}

impl Session {
    pub fn is_live(&self) -> bool {
        self.inner.is_some() && self.exited.is_none()
    }

    pub fn exit_notice(&self) -> Option<String> {
        self.exited.map(|code| match code {
            Some(code) => format!("[process exited with status {code}]"),
            None => "[process was terminated]".to_string(),
        })
    }
}

#[derive(Default)]
pub struct TerminalRequests {
    pub copy: Option<String>,
    pub error: Option<String>,
    pub reload_plugin: bool,
    pub build_failed: bool,
}

pub struct TerminalState {
    port: Rc<dyn TerminalPort>,
    clipboard: Rc<dyn ClipboardPort>,
    palette: TerminalPalette,
    cwd: PathBuf,
    pub sessions: Vec<Session>,
    pub active: usize,
    next_id: u64,
    pub size: TerminalSize,
    pub missing_git_bash: bool,
    opened_once: bool,
    pub scroll_carry: f32,
    pub diagnostics: Vec<Diagnostic>,
    pub stale_files: HashSet<PathBuf>,
}

impl TerminalState {
    pub fn new(
        port: Rc<dyn TerminalPort>,
        clipboard: Rc<dyn ClipboardPort>,
        palette: TerminalPalette,
        cwd: &Path,
    ) -> Self {
        Self {
            port,
            clipboard,
            palette,
            cwd: cwd.to_path_buf(),
            sessions: Vec::new(),
            active: 0,
            next_id: 0,
            size: TerminalSize {
                cols: 80,
                rows: 24,
                cell_width: 8,
                cell_height: 16,
            },
            missing_git_bash: false,
            opened_once: false,
            scroll_carry: 0.0,
            diagnostics: Vec::new(),
            stale_files: HashSet::new(),
        }
    }

    fn take_id(&mut self) -> SessionId {
        self.next_id += 1;
        SessionId(self.next_id)
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.get(self.active)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(self.active)
    }

    fn index_of(&self, id: SessionId) -> Option<usize> {
        self.sessions.iter().position(|s| s.id == id)
    }

    pub fn diagnostic_counts(&self) -> Counts {
        counts(&self.diagnostics)
    }

    pub fn is_stale(&self, path: &Path) -> bool {
        self.stale_files.contains(path)
    }

    pub fn build_status(&self) -> Option<(BuildKind, BuildStatus)> {
        self.sessions.iter().find_map(|s| match (s.kind, s.status) {
            (SessionKind::Build(kind), Some(status)) => Some((kind, status)),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests;
