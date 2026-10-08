//! Draining what every session reported since the last frame.

use super::{SessionKind, TerminalRequests, TerminalState};
use crate::application::ports::TerminalEvent;
use crate::domain::diagnostics::{Diagnostic, parse_diagnostics};
use crate::domain::terminal::{BuildKind, BuildStatus};

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
