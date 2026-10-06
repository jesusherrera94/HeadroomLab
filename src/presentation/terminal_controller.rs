use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::application::ports::{ClipboardPort, TerminalEvent, TerminalPort, TerminalSession};
use crate::domain::diagnostics::{Counts, Diagnostic, counts, parse_diagnostics};
use crate::domain::terminal::{
    BuildKind, BuildStatus, BuildUnavailable, GIT_FOR_WINDOWS_URL, Platform, SCROLLBACK_LINES,
    ShellChoice, TerminalPalette, TerminalSize, TerminalSnapshot, build_command, shell_for,
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

pub fn default_shell() -> ShellChoice {
    shell_for(
        Platform::current(),
        &|key| std::env::var(key).ok(),
        &|path| path.exists(),
    )
}

pub fn open_shell(state: &mut TerminalState) {
    let command = default_shell();
    let title = command.label();
    spawn(state, command, title, SessionKind::Shell);
}

pub fn ensure_open(state: &mut TerminalState) {
    if !state.opened_once {
        state.opened_once = true;
        open_shell(state);
    }
}

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

pub fn close(state: &mut TerminalState, id: SessionId) {
    let Some(index) = state.index_of(id) else {
        return;
    };
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

pub fn clear_active(state: &mut TerminalState) {
    if let Some(inner) = state.active_session().and_then(|s| s.inner.as_ref()) {
        inner.clear();
    }
}

pub fn write_active(state: &TerminalState, bytes: &[u8]) {
    if let Some(session) = state.active_session()
        && session.is_live()
        && let Some(inner) = &session.inner
    {
        inner.scroll(-(SCROLLBACK_LINES as i32));
        inner.write(bytes);
    }
}

pub fn paste_active(state: &TerminalState) {
    if let Some(text) = state.clipboard.read() {
        write_active(state, text.as_bytes());
    }
}

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

pub fn active_snapshot(state: &TerminalState) -> Option<TerminalSnapshot> {
    state
        .active_session()
        .and_then(|s| s.inner.as_ref())
        .map(|inner| inner.snapshot())
}

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
    state.diagnostics.clear();
    state.stale_files.clear();

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

pub fn tick(state: &mut TerminalState) -> TerminalRequests {
    let mut requests = TerminalRequests::default();
    let mut load_clipboard_for: Option<usize> = None;
    let mut parsed: Option<Vec<Diagnostic>> = None;

    for (index, session) in state.sessions.iter_mut().enumerate() {
        let Some(inner) = &session.inner else {
            continue;
        };

        for event in inner.drain_events() {
            match event {
                TerminalEvent::Wakeup | TerminalEvent::Bell => {}

                TerminalEvent::Title(title) if !title.trim().is_empty() => {
                    if matches!(session.kind, SessionKind::Shell) {
                        session.title = title;
                    }
                }
                TerminalEvent::Title(_) => {}

                TerminalEvent::ChildExit(code) => {
                    session.exited = Some(code);
                    if let SessionKind::Build(_) = session.kind {
                        parsed = Some(parse_diagnostics(&inner.logical_text()));
                    }
                    if let SessionKind::Build(kind) = session.kind {
                        let status = match code {
                            Some(0) => BuildStatus::Succeeded,
                            Some(code) => BuildStatus::Failed(code),
                            None => BuildStatus::Failed(-1),
                        };
                        session.status = Some(status);
                        match status {
                            BuildStatus::Succeeded if kind == BuildKind::Dylib => {
                                requests.reload_plugin = true;
                            }
                            BuildStatus::Failed(code) => {
                                requests.build_failed = kind == BuildKind::Dylib;
                                requests.error = Some(format!(
                                    "{} failed with status {code}. See the Build tab for the output.",
                                    kind.label()
                                ));
                            }
                            _ => {}
                        }
                    }
                }

                TerminalEvent::ClipboardStore(text) => requests.copy = Some(text),
                TerminalEvent::ClipboardLoad => load_clipboard_for = Some(index),
            }
        }
    }

    if let Some(diagnostics) = parsed {
        state.diagnostics = diagnostics;
        state.stale_files.clear();
    }

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

    #[test]
    fn opening_and_closing_sessions_keeps_the_active_index_sane() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);

        open_shell(&mut state);
        open_shell(&mut state);
        open_shell(&mut state);
        assert_eq!(state.sessions.len(), 3);
        assert_eq!(state.active, 2, "a new session becomes the active one");

        let first = state.sessions[0].id;
        close(&mut state, first);
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.active, 1);
        assert!(*port.opened.borrow()[0].killed.borrow());

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

        ensure_open(&mut state);
        ensure_open(&mut state);
        ensure_open(&mut state);
        assert_eq!(
            state.sessions.len(),
            1,
            "only the first frame opens a shell"
        );

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

    fn build_finishing_with(kind: BuildKind, code: i32) -> (TerminalRequests, TerminalState) {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        run_build(&mut state, kind);

        let build = port.opened.borrow().last().cloned().expect("build spawned");
        build
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(code)));
        let requests = tick(&mut state);
        (requests, state)
    }

    #[test]
    fn a_build_parses_its_own_output_into_diagnostics() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        run_build(&mut state, BuildKind::Dylib);

        let build = port.opened.borrow().last().cloned().unwrap();
        *build.output.borrow_mut() = "\
effect_processor.cpp:14:24: error: use of undeclared identifier 'cutof'
effect_processor.cpp:9:11: warning: unused variable 'dryMix' [-Wunused-variable]
make: *** [build/libtest3.dylib] Error 1"
            .to_string();
        build
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(2)));
        tick(&mut state);

        assert_eq!(state.diagnostics.len(), 3);
        assert_eq!(
            state.diagnostic_counts(),
            Counts {
                errors: 2,
                warnings: 1
            }
        );
        assert_eq!(state.diagnostics[0].line, Some(14));
    }

    #[test]
    fn a_new_build_clears_the_previous_complaints_before_it_starts() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        run_build(&mut state, BuildKind::Dylib);

        let first = port.opened.borrow().last().cloned().unwrap();
        *first.output.borrow_mut() = "a.cpp:1:1: error: stale".to_string();
        first
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(1)));
        tick(&mut state);
        assert_eq!(state.diagnostics.len(), 1);
        state.stale_files.insert(PathBuf::from("/proj/a.cpp"));

        run_build(&mut state, BuildKind::Dylib);
        assert!(state.diagnostics.is_empty());
        assert!(state.stale_files.is_empty());
    }

    #[test]
    fn a_shell_exiting_never_touches_the_diagnostics() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);
        run_build(&mut state, BuildKind::Dylib);

        let build = port.opened.borrow().last().cloned().unwrap();
        *build.output.borrow_mut() = "a.cpp:1:1: error: real".to_string();
        build
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(1)));
        tick(&mut state);
        assert_eq!(state.diagnostics.len(), 1);

        open_shell(&mut state);
        let shell = port.opened.borrow().last().cloned().unwrap();
        *shell.output.borrow_mut() = "some unrelated shell output".to_string();
        shell
            .events
            .borrow_mut()
            .push(TerminalEvent::ChildExit(Some(0)));
        tick(&mut state);

        assert_eq!(
            state.diagnostics.len(),
            1,
            "only a Build session's exit replaces the diagnostics"
        );
    }

    #[test]
    fn a_successful_dylib_build_asks_for_the_simulator() {
        let (requests, state) = build_finishing_with(BuildKind::Dylib, 0);
        assert!(requests.reload_plugin);
        assert!(requests.error.is_none());
        assert_eq!(
            state.build_status(),
            Some((BuildKind::Dylib, BuildStatus::Succeeded))
        );
    }

    #[test]
    fn a_failed_build_reports_instead_of_launching() {
        let (requests, state) = build_finishing_with(BuildKind::Dylib, 2);
        assert!(
            !requests.reload_plugin,
            "a library that failed to build must not be loaded"
        );
        assert!(requests.error.is_some_and(|e| e.contains("status 2")));
        assert_eq!(
            state.build_status(),
            Some((BuildKind::Dylib, BuildStatus::Failed(2)))
        );
    }

    #[test]
    fn a_failed_dylib_build_asks_for_the_waiting_emulator_to_be_discarded() {
        let (requests, _) = build_finishing_with(BuildKind::Dylib, 2);
        assert!(requests.build_failed);
        assert!(!requests.reload_plugin);
    }

    #[test]
    fn a_failed_firmware_build_leaves_the_emulator_alone() {
        let (requests, _) = build_finishing_with(BuildKind::Firmware, 2);
        assert!(!requests.build_failed);
        assert!(!requests.reload_plugin);
    }

    #[test]
    fn a_successful_build_never_reports_failure() {
        let (requests, _) = build_finishing_with(BuildKind::Dylib, 0);
        assert!(!requests.build_failed);
        assert!(requests.reload_plugin);
    }

    #[test]
    fn compiling_firmware_never_opens_the_simulator() {
        let (requests, _) = build_finishing_with(BuildKind::Firmware, 0);
        assert!(!requests.reload_plugin);
    }

    #[test]
    fn building_twice_reuses_the_build_tab_and_kills_what_was_running() {
        let port = Rc::new(FakePort::default());
        let mut state = state_over(port.clone(), None);

        run_build(&mut state, BuildKind::Dylib);
        let first = port.opened.borrow().last().cloned().unwrap();

        run_build(&mut state, BuildKind::Firmware);

        assert!(
            *first.killed.borrow(),
            "a stale build is worthless once the button is pressed again"
        );
        assert_eq!(
            state
                .sessions
                .iter()
                .filter(|s| matches!(s.kind, SessionKind::Build(_)))
                .count(),
            1,
            "repeated builds must not pile up tabs"
        );
        assert_eq!(
            state.build_status(),
            Some((BuildKind::Firmware, BuildStatus::Running))
        );
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
