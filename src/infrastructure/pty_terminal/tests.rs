use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::domain::terminal::{Rgb, indexed_color};

fn test_palette() -> TerminalPalette {
    TerminalPalette {
        named: [Rgb::new(0, 0, 0); 16],
        foreground: Rgb::new(0xcc, 0xcc, 0xcc),
        background: Rgb::new(0x11, 0x11, 0x18),
        cursor: Rgb::new(0xcc, 0xcc, 0xcc),
    }
}

fn size() -> TerminalSize {
    TerminalSize {
        cols: 40,
        rows: 8,
        cell_width: 8,
        cell_height: 16,
    }
}

fn screen_text(session: &dyn TerminalSession) -> String {
    let snapshot = session.snapshot();
    (0..snapshot.rows)
        .map(|row| {
            snapshot
                .row(row)
                .iter()
                .map(|cell| cell.c)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn wait_for(
    session: &dyn TerminalSession,
    events: &mut Vec<TerminalEvent>,
    mut done: impl FnMut(&dyn TerminalSession, &[TerminalEvent]) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        events.extend(session.drain_events());
        if done(session, events) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
#[cfg(unix)]
fn a_session_runs_a_command_and_reports_its_exit_status() {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = woken.clone();
    let terminal = PtyTerminal::new(Arc::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    }));

    let shell = ShellChoice::new("/bin/echo", &["hello from the pty"]);
    let session = terminal
        .open(&shell, Path::new("/"), size(), test_palette())
        .expect("PTY should open");

    let mut events = Vec::new();
    let finished = wait_for(session.as_ref(), &mut events, |session, events| {
        screen_text(session).contains("hello from the pty")
            && events
                .iter()
                .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
    });

    assert!(
        finished,
        "expected the output and an exit event; grid was {:?}, events were {events:?}",
        screen_text(session.as_ref())
    );
    assert!(
        events.contains(&TerminalEvent::ChildExit(Some(0))),
        "echo should exit 0, got {events:?}"
    );
    assert!(
        woken.load(Ordering::Relaxed) > 0,
        "the UI must be woken when output arrives, or the panel never repaints"
    );
}

#[test]
#[cfg(unix)]
fn output_survives_the_child_that_produced_it() {
    let terminal = PtyTerminal::new(Arc::new(|| {}));
    let shell = ShellChoice::new(
        "/bin/sh",
        &["-c", "echo error: undefined reference; exit 2"],
    );
    let session = terminal
        .open(&shell, Path::new("/"), size(), test_palette())
        .expect("PTY should open");

    let mut events = Vec::new();
    let finished = wait_for(session.as_ref(), &mut events, |session, events| {
        events
            .iter()
            .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
            && screen_text(session).contains("undefined reference")
    });

    assert!(
        finished,
        "the error line must outlive the process; grid was {:?}",
        screen_text(session.as_ref())
    );

    assert!(screen_text(session.as_ref()).contains("error: undefined reference"));
    assert!(events.contains(&TerminalEvent::ChildExit(Some(2))));
}

#[test]
#[cfg(unix)]
fn scrolling_back_shows_history_instead_of_a_blank_gap() {
    let terminal = PtyTerminal::new(Arc::new(|| {}));
    let shell = ShellChoice::new(
        "/bin/sh",
        &[
            "-c",
            "i=1; while [ $i -le 30 ]; do echo \"line $i\"; i=$((i+1)); done",
        ],
    );
    let session = terminal
        .open(&shell, Path::new("/"), size(), test_palette())
        .expect("PTY should open");

    let mut events = Vec::new();
    let finished = wait_for(session.as_ref(), &mut events, |session, events| {
        events
            .iter()
            .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
            && screen_text(session).contains("line 30")
    });
    assert!(
        finished,
        "the tail of the output should be on screen; grid was {:?}",
        screen_text(session.as_ref())
    );

    session.scroll(10);

    let scrolled = screen_text(session.as_ref());
    assert!(
        !scrolled.contains("line 30"),
        "scrolling back must move the tail off screen; grid was {scrolled:?}"
    );
    assert!(
        scrolled.contains("line 15"),
        "scrolling back must bring history into view; grid was {scrolled:?}"
    );
    assert!(
        !scrolled.lines().last().unwrap_or_default().is_empty(),
        "the bottom rows must hold scrolled content, not a blank gap; grid was {scrolled:?}"
    );
}

#[test]
#[cfg(unix)]
fn a_line_wider_than_the_grid_is_rejoined_for_parsing() {
    use crate::domain::diagnostics::{Severity, parse_diagnostics};

    let terminal = PtyTerminal::new(Arc::new(|| {}));
    let long_path = "src/very/deeply/nested/directory/effect_processor.cpp";
    let message = "use of undeclared identifier 'cutof'; did you mean 'cutoff'?";
    let line = format!("{long_path}:14:24: error: {message}");
    let shell = ShellChoice::new("/bin/sh", &["-c", &format!("printf '%s\\n' \"{line}\"")]);

    let session = terminal
        .open(&shell, Path::new("/"), size(), test_palette())
        .expect("PTY should open");

    let mut events = Vec::new();
    let finished = wait_for(session.as_ref(), &mut events, |session, events| {
        events
            .iter()
            .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
            && !parse_diagnostics(&session.logical_text()).is_empty()
    });
    assert!(
        finished,
        "nothing parsed; logical text was {:?}",
        session.logical_text()
    );

    let rendered = screen_text(session.as_ref());
    assert!(
        !rendered.lines().any(|row| row.contains(&line)),
        "expected the grid to have wrapped the line, but a row held it whole"
    );

    let parsed = parse_diagnostics(&session.logical_text());
    assert_eq!(parsed.len(), 1, "got {parsed:?}");
    assert_eq!(parsed[0].severity, Severity::Error);
    assert_eq!(parsed[0].file.as_deref(), Some(long_path));
    assert_eq!(parsed[0].line, Some(14));
    assert_eq!(parsed[0].column, Some(24));
    assert_eq!(parsed[0].message, message);
}

#[test]
#[cfg(unix)]
fn a_failing_command_reports_its_non_zero_status() {
    let terminal = PtyTerminal::new(Arc::new(|| {}));
    let shell = ShellChoice::new("/bin/sh", &["-c", "exit 3"]);
    let session = terminal
        .open(&shell, Path::new("/"), size(), test_palette())
        .expect("PTY should open");

    let mut events = Vec::new();
    let finished = wait_for(session.as_ref(), &mut events, |_, events| {
        events
            .iter()
            .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
    });

    assert!(finished, "no exit event arrived: {events:?}");
    assert!(
        events.contains(&TerminalEvent::ChildExit(Some(3))),
        "the build's real exit code is what drives the status, got {events:?}"
    );
}

#[test]
#[cfg(unix)]
fn the_real_default_shell_opens_accepts_input_and_reports_its_exit_code() {
    use crate::domain::terminal::{Platform, shell_for};

    let shell = shell_for(
        Platform::current(),
        &|key| std::env::var(key).ok(),
        &|path| path.exists(),
    );
    let terminal = PtyTerminal::new(Arc::new(|| {}));
    let session = terminal
        .open(&shell, Path::new("/"), size(), test_palette())
        .unwrap_or_else(|e| panic!("{} should open: {e}", shell.program));

    session.write(b"exit 7\n");

    let mut events = Vec::new();
    let finished = wait_for(session.as_ref(), &mut events, |_, events| {
        events
            .iter()
            .any(|e| matches!(e, TerminalEvent::ChildExit(_)))
    });

    assert!(finished, "{} never exited: {events:?}", shell.program);
    assert!(
        events.contains(&TerminalEvent::ChildExit(Some(7))),
        "input must reach the shell and its status come back, got {events:?}"
    );
}

#[test]
fn opening_a_missing_program_fails_with_its_name() {
    let terminal = PtyTerminal::new(Arc::new(|| {}));
    let shell = ShellChoice::new("/nonexistent/definitely-not-a-shell", &[]);
    let opened = terminal.open(&shell, Path::new("/"), size(), test_palette());

    if let Err(error) = opened {
        assert!(
            error.0.contains("definitely-not-a-shell"),
            "the error should name the program: {error}"
        );
    }
}

#[test]
fn the_xterm_cube_and_greyscale_ramp_resolve_to_known_values() {
    let palette = test_palette();
    // 16 is the cube's origin: pure black.
    assert_eq!(indexed_color(16, &palette), Rgb::new(0, 0, 0));
    // 231 is its opposite corner: pure white.
    assert_eq!(indexed_color(231, &palette), Rgb::new(255, 255, 255));
    // 196 is the classic bright red of the cube.
    assert_eq!(indexed_color(196, &palette), Rgb::new(255, 0, 0));
    // The greyscale ramp starts at 8 and steps by 10.
    assert_eq!(indexed_color(232, &palette), Rgb::new(8, 8, 8));
    assert_eq!(indexed_color(255, &palette), Rgb::new(238, 238, 238));
}
