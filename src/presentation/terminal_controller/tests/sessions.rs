use super::*;

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
