use super::*;

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
