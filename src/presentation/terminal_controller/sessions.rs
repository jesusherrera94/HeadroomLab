//! Opening, closing, restarting and talking to terminal sessions.

use super::{Session, SessionId, SessionKind, TerminalState};
use crate::domain::terminal::{
    BuildStatus, Platform, SCROLLBACK_LINES, ShellChoice, TerminalSize, TerminalSnapshot, shell_for,
};

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

pub(super) fn spawn(
    state: &mut TerminalState,
    command: ShellChoice,
    title: String,
    kind: SessionKind,
) {
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
