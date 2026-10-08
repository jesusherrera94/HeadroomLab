use super::*;

#[test]
fn a_tab_that_goes_away_hands_back_its_id_so_its_editor_state_can_be_dropped() {
    let path = PathBuf::from("/proj/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
    let mut state = state_over(fs.clone());

    open_tab(&mut state, &path);
    let closed = tab_id(&state, 0);
    request_close_tab(&mut state, closed);

    assert_eq!(take_discarded_tabs(&mut state), vec![closed]);
    assert!(
        take_discarded_tabs(&mut state).is_empty(),
        "draining twice must not act on the same id twice"
    );

    open_tab(&mut state, &path);
    let pruned = tab_id(&state, 0);
    assert_ne!(pruned, closed, "ids are never reused");
    fs.files.borrow_mut().remove(&path);
    prune_missing(&mut state);

    assert!(state.tabs.is_empty());
    assert_eq!(take_discarded_tabs(&mut state), vec![pruned]);
}

#[test]
fn closing_a_dirty_tab_asks_before_discarding() {
    let path = PathBuf::from("/proj/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
    let mut state = state_over(fs.clone());

    open_tab(&mut state, &path);
    let clean = tab_id(&state, 0);
    request_close_tab(&mut state, clean);
    assert!(state.tabs.is_empty(), "a clean tab closes immediately");

    open_tab(&mut state, &path);
    edit_active(&mut state, "two\n");
    let dirty = tab_id(&state, 0);
    request_close_tab(&mut state, dirty);
    assert_eq!(state.tabs.len(), 1, "a dirty tab waits for confirmation");
    assert!(state.explorer.pending_confirm.is_some());

    run_pending_confirm(&mut state, ConfirmChoice::Alternate);
    assert!(state.tabs.is_empty());
    assert_eq!(fs.read_file(&path).unwrap(), b"two\n".to_vec());
}

#[test]
fn quitting_is_guarded_only_while_work_is_unsaved() {
    let path = PathBuf::from("/proj/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
    let mut state = state_over(fs.clone());

    open_tab(&mut state, &path);
    assert!(
        !request_quit(&mut state),
        "nothing dirty → quit straight away"
    );

    edit_active(&mut state, "two\n");
    assert!(request_quit(&mut state));
    assert!(state.explorer.pending_confirm.is_some());

    assert!(run_pending_confirm(&mut state, ConfirmChoice::Alternate).quit);
    assert_eq!(fs.read_file(&path).unwrap(), b"two\n".to_vec());
    assert!(!has_unsaved_work(&state));
}

#[test]
fn move_tab_reorders_and_keeps_the_active_buffer() {
    let (_fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp", "c.cpp"]);
    state.active_tab = 1;
    let active = tab_id(&state, 1);

    move_tab(&mut state, 0, 2);
    assert_eq!(tab_names(&state), ["b.cpp", "a.cpp", "c.cpp"]);

    assert_eq!(state.tabs[state.active_tab].id, active);
    assert_eq!(state.tabs[state.active_tab].name, "b.cpp");
    assert_eq!(state.tabs[state.active_tab].content.text(), Some("b.cpp"));

    move_tab(&mut state, 0, 3);
    assert_eq!(tab_names(&state), ["a.cpp", "c.cpp", "b.cpp"]);
    assert_eq!(state.tabs[state.active_tab].id, active);
}

#[test]
fn move_tab_ignores_out_of_range_and_no_op_moves() {
    let (_fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp", "c.cpp"]);

    for (from, insert_before) in [(0, 0), (1, 1), (1, 2), (3, 0), (0, 4), (9, 9)] {
        move_tab(&mut state, from, insert_before);
        assert_eq!(
            tab_names(&state),
            ["a.cpp", "b.cpp", "c.cpp"],
            "move_tab({from}, {insert_before}) should have been a no-op"
        );
    }
}

#[test]
fn close_confirm_targets_the_tab_by_identity_not_slot() {
    let (fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp", "c.cpp"]);

    state.active_tab = 2;
    edit_active(&mut state, "edited c\n");
    let doomed = tab_id(&state, 2);
    request_close_tab(&mut state, doomed);
    assert!(state.explorer.pending_confirm.is_some());

    move_tab(&mut state, 2, 0);
    fs.files.borrow_mut().remove(Path::new("/proj/a.cpp"));
    prune_missing(&mut state);
    assert_eq!(tab_names(&state), ["c.cpp", "b.cpp"]);

    run_pending_confirm(&mut state, ConfirmChoice::Primary);
    assert_eq!(tab_names(&state), ["b.cpp"]);
    assert_eq!(
        fs.read_file(Path::new("/proj/c.cpp")).unwrap(),
        b"c.cpp".to_vec(),
        "discarding must not have written the buffer"
    );
}

#[test]
fn close_confirm_no_ops_when_its_tab_is_already_gone() {
    let (fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp"]);

    state.active_tab = 0;
    edit_active(&mut state, "edited a\n");
    let doomed = tab_id(&state, 0);
    request_close_tab(&mut state, doomed);

    save_tab(&mut state, 0);
    close_tab(&mut state, 0);
    assert_eq!(tab_names(&state), ["b.cpp"]);

    run_pending_confirm(&mut state, ConfirmChoice::Primary);
    assert_eq!(
        tab_names(&state),
        ["b.cpp"],
        "a confirmation whose tab is gone must not close a bystander"
    );
    assert_eq!(
        fs.read_file(Path::new("/proj/a.cpp")).unwrap(),
        b"edited a\n"
    );
}

#[test]
fn unsaved_paths_tracks_the_dirty_set() {
    let (_fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp"]);
    assert!(unsaved_paths(&state).is_empty());

    state.active_tab = 1;
    edit_active(&mut state, "edited b\n");
    assert_eq!(
        unsaved_paths(&state),
        HashSet::from([PathBuf::from("/proj/b.cpp")])
    );

    move_tab(&mut state, 1, 0);
    assert_eq!(
        unsaved_paths(&state),
        HashSet::from([PathBuf::from("/proj/b.cpp")])
    );
    assert!(save_tab(&mut state, 0));
    assert!(unsaved_paths(&state).is_empty());
}

#[test]
fn saving_a_background_tab_leaves_the_active_tab_alone() {
    let (fs, mut state) = state_with_tabs(&["a.cpp", "b.cpp"]);

    state.active_tab = 0;
    edit_active(&mut state, "edited a\n");
    state.active_tab = 1;

    let background = tab_id(&state, 0);
    handle_events(
        &mut state,
        EditorViewEvents {
            tab_saved: Some(background),
            ..Default::default()
        },
    );

    assert_eq!(
        fs.read_file(Path::new("/proj/a.cpp")).unwrap(),
        b"edited a\n"
    );
    assert!(!state.tabs[0].unsaved());
    assert_eq!(
        state.active_tab, 1,
        "pressing a background tab's ● must not steal the code pane"
    );
}
