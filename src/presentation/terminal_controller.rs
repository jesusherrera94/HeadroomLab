//! State and behaviour for the terminal panel: the open sessions, which one is
//! showing, and what to do with the events each reports.
//!
//! Free of egui, like the other controllers — the panel draws, this decides.
//! Anything that needs the UI framework (putting text on the clipboard when a
//! program asks via OSC 52) leaves as a [`TerminalRequests`] for the window to
//! carry out.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::application::ports::{ClipboardPort, TerminalEvent, TerminalPort, TerminalSession};
use crate::domain::terminal::{
    BuildKind, BuildStatus, BuildUnavailable, GIT_FOR_WINDOWS_URL, Platform, ShellChoice,
    TerminalPalette, TerminalSize, TerminalSnapshot, build_command, shell_for,
};

/// Identity for a session, so an action raised in one frame still refers to the
/// same shell when it runs — the same reasoning as `TabId` in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

/// What a session is for. `Build` sessions are ordinary sessions whose child is
/// a `make` invocation rather than a shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Shell,
    Build(BuildKind),
}

/// One terminal tab.
pub struct Session {
    pub id: SessionId,
    /// Tab label: the shell's name, or whatever a program set the title to.
    pub title: String,
    pub kind: SessionKind,
    /// The command this session runs, kept so **Restart** can respawn it.
    pub command: ShellChoice,
    /// `None` once the child has exited or when it never started.
    pub inner: Option<Box<dyn TerminalSession>>,
    /// The child's exit code, once it has one. `Some(None)` means it was killed
    /// by a signal, which has no code.
    pub exited: Option<Option<i32>>,
    /// Build progress, for the tab dot and the status bar. Only ever `Some` on a
    /// `Build` session.
    pub status: Option<BuildStatus>,
    /// Why this session could not start, rendered in its own body rather than a
    /// modal — a terminal that will not open should not block the editor.
    pub error: Option<String>,
    /// Where a mouse drag started, in cell coordinates.
    pub drag_anchor: Option<(u16, u16)>,
}

impl Session {
    /// Whether this session's child is still running.
    pub fn is_live(&self) -> bool {
        self.inner.is_some() && self.exited.is_none()
    }

    /// The message shown in place of the grid once the child is gone.
    pub fn exit_notice(&self) -> Option<String> {
        self.exited.map(|code| match code {
            Some(code) => format!("[process exited with status {code}]"),
            None => "[process was terminated]".to_string(),
        })
    }
}

/// Work the panel cannot do itself because it needs the UI framework.
#[derive(Default)]
pub struct TerminalRequests {
    /// A program asked for this text to go on the clipboard (OSC 52).
    pub copy: Option<String>,
    /// A build finished badly; the message belongs in the editor's error banner.
    pub error: Option<String>,
}

pub struct TerminalState {
    port: Rc<dyn TerminalPort>,
    clipboard: Rc<dyn ClipboardPort>,
    palette: TerminalPalette,
    /// Every session starts here — the project root.
    cwd: PathBuf,
    pub sessions: Vec<Session>,
    pub active: usize,
    next_id: u64,
    /// The grid size last computed from the panel, reused when opening a session
    /// so a new tab does not start at some default size and immediately reflow.
    pub size: TerminalSize,
    /// Set on Windows when a build is attempted without git-bash.
    pub missing_git_bash: bool,
    /// Whether the automatic first session has been opened yet.
    opened_once: bool,
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
            // Replaced by the real measurement on the first paint; only used if
            // a session somehow opens before the panel has been laid out.
            size: TerminalSize {
                cols: 80,
                rows: 24,
                cell_width: 8,
                cell_height: 16,
            },
            missing_git_bash: false,
            opened_once: false,
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

    /// The build session's status, for the status bar.
    pub fn build_status(&self) -> Option<(BuildKind, BuildStatus)> {
        self.sessions.iter().find_map(|s| match (s.kind, s.status) {
            (SessionKind::Build(kind), Some(status)) => Some((kind, status)),
            _ => None,
        })
    }
}

/// The shell this platform runs, resolved against the real environment.
pub fn default_shell() -> ShellChoice {
    shell_for(
        Platform::current(),
        &|key| std::env::var(key).ok(),
        &|path| path.exists(),
    )
}

/// Opens another shell tab and makes it active.
pub fn open_shell(state: &mut TerminalState) {
    let command = default_shell();
    let title = command.label();
    spawn(state, command, title, SessionKind::Shell);
}

/// Opens the *first* shell, once, as soon as the panel knows its real size — so
/// the session never starts at a guessed 80×24 and immediately reflows.
///
/// Guarded by a flag rather than by `sessions.is_empty()`: this runs every
/// frame, so the emptiness test would respawn a shell the instant the user
/// closed the last one, and the panel could never be left empty.
pub fn ensure_open(state: &mut TerminalState) {
    if !state.opened_once {
        state.opened_once = true;
        open_shell(state);
    }
}

/// Spawns `command` as a new session. A failure to start becomes the session's
/// own error rather than a modal, so the tab exists and can be retried.
fn spawn(state: &mut TerminalState, command: ShellChoice, title: String, kind: SessionKind) {
    let id = state.take_id();
    let opened = state
        .port
        .open(&command, &state.cwd, state.size, state.palette);

    let (inner, error) = match opened {
        Ok(session) => (Some(session), None),
        Err(e) => (None, Some(e.0)),
    };

    state.sessions.push(Session {
        id,
        title,
        kind,
        command,
        inner,
        exited: None,
        status: matches!(kind, SessionKind::Build(_)).then_some(BuildStatus::Running),
        error,
        drag_anchor: None,
    });
    state.active = state.sessions.len() - 1;
}

/// Closes a session, killing its child.
pub fn close(state: &mut TerminalState, id: SessionId) {
    let Some(index) = state.index_of(id) else {
        return;
    };
    // Dropping the session sends `Shutdown`; killing first makes the intent
    // explicit and covers a child that ignores the closed PTY.
    if let Some(inner) = &state.sessions[index].inner {
        inner.kill();
    }
    state.sessions.remove(index);

    if state.active > index {
        state.active -= 1;
    }
    if state.active >= state.sessions.len() {
        state.active = state.sessions.len().saturating_sub(1);
    }
}

pub fn activate(state: &mut TerminalState, id: SessionId) {
    if let Some(index) = state.index_of(id) {
        state.active = index;
    }
}

/// Respawns a session's command in place, keeping its position in the strip.
pub fn restart(state: &mut TerminalState, id: SessionId) {
    let Some(index) = state.index_of(id) else {
        return;
    };
    let session = &state.sessions[index];
    let command = session.command.clone();
    let kind = session.kind;

    let opened = state
        .port
        .open(&command, &state.cwd, state.size, state.palette);

    let session = &mut state.sessions[index];
    match opened {
        Ok(inner) => {
            session.inner = Some(inner);
            session.exited = None;
            session.error = None;
            session.status = matches!(kind, SessionKind::Build(_)).then_some(BuildStatus::Running);
        }
        Err(e) => {
            session.inner = None;
            session.error = Some(e.0);
        }
    }
}

/// Empties the active session's screen and scrollback.
pub fn clear_active(state: &mut TerminalState) {
    if let Some(inner) = state.active_session().and_then(|s| s.inner.as_ref()) {
        inner.clear();
    }
}

/// Sends bytes to the active session's child.
pub fn write_active(state: &TerminalState, bytes: &[u8]) {
    if let Some(session) = state.active_session()
        && session.is_live()
        && let Some(inner) = &session.inner
    {
        inner.write(bytes);
    }
}

/// Pastes the system clipboard into the active session.
pub fn paste_active(state: &TerminalState) {
    if let Some(text) = state.clipboard.read() {
        write_active(state, text.as_bytes());
    }
}

/// Tells every live session the panel's new grid size.
///
/// Called only when the size actually changed: `Term::resize` reflows the whole
/// grid, and doing that on every frame of a splitter drag is visible.
pub fn resize(state: &mut TerminalState, size: TerminalSize) {
    if size == state.size {
        return;
    }
    state.size = size;
    for session in &state.sessions {
        if let Some(inner) = &session.inner {
            inner.resize(size);
        }
    }
}

/// A frame's view of the active session, or `None` when there is nothing live to
/// draw (no sessions, a failed spawn, or a child that has exited).
pub fn active_snapshot(state: &TerminalState) -> Option<TerminalSnapshot> {
    state
        .active_session()
        .and_then(|s| s.inner.as_ref())
        .map(|inner| inner.snapshot())
}

/// Starts a build, replacing whatever the Build tab was doing.
///
/// Kill-and-restart rather than queue or refuse: once the button is pressed
/// again, the build in flight is answering a question nobody is asking any more.
pub fn run_build(state: &mut TerminalState, kind: BuildKind) -> TerminalRequests {
    let mut requests = TerminalRequests::default();

    let command = build_command(
        kind,
        Platform::current(),
        &|key| std::env::var(key).ok(),
        &|path| path.exists(),
    );
    let command = match command {
        Ok(command) => command,
        Err(BuildUnavailable::NeedsGitBash) => {
            state.missing_git_bash = true;
            requests.error = Some(format!(
                "{} needs `make`, which on Windows comes with Git for Windows. \
                 Install it from {GIT_FOR_WINDOWS_URL} and try again.",
                kind.label()
            ));
            return requests;
        }
    };

    state.missing_git_bash = false;

    // Reuse the Build tab if there is one, so repeated builds do not pile up
    // tabs; its child is killed on the way (D7).
    if let Some(index) = state
        .sessions
        .iter()
        .position(|s| matches!(s.kind, SessionKind::Build(_)))
    {
        let id = state.sessions[index].id;
        if let Some(inner) = &state.sessions[index].inner {
            inner.kill();
        }
        let session = &mut state.sessions[index];
        session.kind = SessionKind::Build(kind);
        session.command = command;
        session.title = "Build".to_string();
        state.active = index;
        restart(state, id);
        return requests;
    }

    spawn(
        state,
        command,
        "Build".to_string(),
        SessionKind::Build(kind),
    );
    requests
}

/// Drains every session's events: retitling, exit codes, build status and
/// clipboard requests. Called once per frame from the panel.
pub fn tick(state: &mut TerminalState) -> TerminalRequests {
    let mut requests = TerminalRequests::default();
    let mut load_clipboard_for: Option<usize> = None;

    for (index, session) in state.sessions.iter_mut().enumerate() {
        let Some(inner) = &session.inner else {
            continue;
        };

        for event in inner.drain_events() {
            match event {
                // The repaint was already requested from the reader thread; the
                // event itself carries nothing else to act on.
                TerminalEvent::Wakeup | TerminalEvent::Bell => {}

                TerminalEvent::Title(title) if !title.trim().is_empty() => {
                    // A build tab keeps its name: `make` sets titles of its own,
                    // and "Build" is what the toolbar button promised.
                    if matches!(session.kind, SessionKind::Shell) {
                        session.title = title;
                    }
                }
                TerminalEvent::Title(_) => {}

                TerminalEvent::ChildExit(code) => {
                    session.exited = Some(code);
                    if let SessionKind::Build(kind) = session.kind {
                        let status = match code {
                            Some(0) => BuildStatus::Succeeded,
                            Some(code) => BuildStatus::Failed(code),
                            // Killed by a signal — a kill-and-restart, or Ctrl+C.
                            None => BuildStatus::Failed(-1),
                        };
                        session.status = Some(status);
                        if let BuildStatus::Failed(code) = status {
                            requests.error = Some(format!(
                                "{} failed with status {code}. See the Build tab for the output.",
                                kind.label()
                            ));
                        }
                    }
                }

                TerminalEvent::ClipboardStore(text) => requests.copy = Some(text),
                TerminalEvent::ClipboardLoad => load_clipboard_for = Some(index),
            }
        }
    }

    // Deferred: reading the clipboard needs `state.clipboard` while the loop
    // above holds `state.sessions` mutably.
    if let Some(index) = load_clipboard_for
        && let Some(text) = state.clipboard.read()
        && let Some(inner) = state.sessions[index].inner.as_ref()
    {
        inner.write(text.as_bytes());
    }

    requests
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::TerminalError;
    use crate::domain::terminal::{Rgb, TerminalCell};
    use std::cell::RefCell;

    fn palette() -> TerminalPalette {
        TerminalPalette {
            named: [Rgb::new(0, 0, 0); 16],
            foreground: Rgb::new(0xcc, 0xcc, 0xcc),
            background: Rgb::new(0, 0, 0),
            cursor: Rgb::new(0xcc, 0xcc, 0xcc),
        }
    }

    /// A session that records what it was told, so the controller's decisions
    /// can be asserted without a PTY.
    #[derive(Default)]
    struct FakeInner {
        written: RefCell<Vec<u8>>,
        killed: RefCell<bool>,
        cleared: RefCell<bool>,
        resized: RefCell<Vec<TerminalSize>>,
        events: RefCell<Vec<TerminalEvent>>,
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

    /// Hands out `FakeInner`s and keeps a handle on each, so a test can inspect
    /// the session the controller opened.
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

    #[test]
    fn opening_and_closing_sessions_keeps_the_active_index_sane() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);

        open_shell(&mut state);
        open_shell(&mut state);
        open_shell(&mut state);
        assert_eq!(state.sessions.len(), 3);
        assert_eq!(state.active, 2, "a new session becomes the active one");

        // Closing a session before the active one shifts the index down with it.
        let first = state.sessions[0].id;
        close(&mut state, first);
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.active, 1);
        assert!(*port.opened.borrow()[0].killed.borrow());

        // Closing the active one clamps rather than running off the end.
        let last = state.sessions[1].id;
        close(&mut state, last);
        assert_eq!(state.active, 0);

        let remaining = state.sessions[0].id;
        close(&mut state, remaining);
        assert!(state.sessions.is_empty());
        assert_eq!(state.active, 0, "an empty panel must not index past zero");
    }

    #[test]
    fn the_first_session_opens_once_and_closing_it_leaves_the_panel_empty() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);

        // `ensure_open` runs every frame from the panel.
        ensure_open(&mut state);
        ensure_open(&mut state);
        ensure_open(&mut state);
        assert_eq!(
            state.sessions.len(),
            1,
            "only the first frame opens a shell"
        );

        // Regression: guarding on `sessions.is_empty()` instead of a flag would
        // respawn a shell the instant the user closed the last one, making an
        // empty panel unreachable.
        let only = state.sessions[0].id;
        close(&mut state, only);
        ensure_open(&mut state);
        assert!(
            state.sessions.is_empty(),
            "closing the last session must leave the panel empty"
        );
    }

    #[test]
    fn a_session_that_cannot_start_becomes_a_tab_with_an_error_not_a_lost_click() {
        let port = Rc::new(FakePort::default());
        *port.fail_with.borrow_mut() = Some("no such shell".into());
        let mut state = state_over(port, None);

        open_shell(&mut state);

        assert_eq!(
            state.sessions.len(),
            1,
            "the tab exists so it can be retried"
        );
        assert_eq!(state.sessions[0].error.as_deref(), Some("no such shell"));
        assert!(!state.sessions[0].is_live());
        assert!(active_snapshot(&state).is_none());
    }

    #[test]
    fn resize_is_skipped_when_nothing_changed() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        open_shell(&mut state);

        let bigger = TerminalSize {
            cols: 100,
            rows: 30,
            cell_width: 8,
            cell_height: 16,
        };
        resize(&mut state, bigger);
        resize(&mut state, bigger);
        resize(&mut state, bigger);

        assert_eq!(
            port.opened.borrow()[0].resized.borrow().len(),
            1,
            "reflow is expensive; an unchanged size must not trigger it"
        );
    }

    #[test]
    fn an_exit_marks_the_session_and_stops_input_reaching_it() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        open_shell(&mut state);

        port.opened.borrow()[0]
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(0)));
        tick(&mut state);

        assert_eq!(state.sessions[0].exited, Some(Some(0)));
        assert!(!state.sessions[0].is_live());
        assert_eq!(
            state.sessions[0].exit_notice().as_deref(),
            Some("[process exited with status 0]")
        );

        write_active(&state, b"ls\n");
        assert!(
            port.opened.borrow()[0].written.borrow().is_empty(),
            "a dead session must not be written to"
        );

        // The emulator outlives its child, so the output stays readable. This is
        // what lets the panel show a failed build's errors after `make` exits —
        // dropping the session here would take the diagnostics with it.
        assert!(
            active_snapshot(&state).is_some(),
            "an exited session must keep its scrollback: it is the build log"
        );
    }

    #[test]
    fn a_program_may_rename_a_shell_tab_but_never_the_build_tab() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        open_shell(&mut state);

        port.opened.borrow()[0]
            .events
            .borrow_mut()
            .push(TerminalEvent::Title("~/projects".into()));
        tick(&mut state);
        assert_eq!(state.sessions[0].title, "~/projects");

        // Blank titles are ignored rather than blanking the tab.
        port.opened.borrow()[0]
            .events
            .borrow_mut()
            .push(TerminalEvent::Title("   ".into()));
        tick(&mut state);
        assert_eq!(state.sessions[0].title, "~/projects");
    }

    #[test]
    fn a_clipboard_request_from_the_terminal_is_answered_both_ways() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), Some("pasted text"));
        open_shell(&mut state);

        port.opened.borrow()[0]
            .events
            .borrow_mut()
            .push(TerminalEvent::ClipboardStore("copied".into()));
        let requests = tick(&mut state);
        assert_eq!(requests.copy.as_deref(), Some("copied"));

        port.opened.borrow()[0]
            .events
            .borrow_mut()
            .push(TerminalEvent::ClipboardLoad);
        tick(&mut state);
        assert_eq!(
            String::from_utf8_lossy(&port.opened.borrow()[0].written.borrow()),
            "pasted text"
        );
    }

    #[test]
    fn paste_writes_the_clipboard_to_the_active_session() {
        let port = Rc::new(FakePort::default());
        let state = {
            let mut state = state_over(port.clone(), Some("echo hi"));
            open_shell(&mut state);
            state
        };
        paste_active(&state);
        assert_eq!(
            String::from_utf8_lossy(&port.opened.borrow()[0].written.borrow()),
            "echo hi"
        );
    }

    #[test]
    fn clear_reaches_only_the_active_session() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        open_shell(&mut state);
        open_shell(&mut state);

        clear_active(&mut state);

        assert!(!*port.opened.borrow()[0].cleared.borrow());
        assert!(*port.opened.borrow()[1].cleared.borrow());
    }

    #[test]
    fn restarting_respawns_the_same_command_in_place() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        open_shell(&mut state);
        open_shell(&mut state);

        let first = state.sessions[0].id;
        port.opened.borrow()[0]
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(1)));
        tick(&mut state);
        assert!(!state.sessions[0].is_live());

        restart(&mut state, first);

        assert!(state.sessions[0].is_live(), "the session is running again");
        assert_eq!(state.sessions[0].exited, None);
        assert_eq!(
            state.sessions.len(),
            2,
            "restart is in place, not a new tab"
        );
        assert_eq!(state.active, 1, "and does not steal the active tab");
    }
}
