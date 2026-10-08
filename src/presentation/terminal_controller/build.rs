//! Running a build in the dedicated Build tab.

use super::sessions::{restart, spawn};
use super::{SessionKind, TerminalRequests, TerminalState};
use crate::domain::terminal::{
    BuildKind, BuildUnavailable, GIT_FOR_WINDOWS_URL, Platform, build_command,
};

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
