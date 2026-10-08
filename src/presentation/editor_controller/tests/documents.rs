use super::*;

#[test]
fn open_document_classifies_and_save_round_trips_crlf() {
    use crate::domain::text_document::DocumentContent;

    let path = PathBuf::from("/proj/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"int a;\r\nint b;\r\n"));
    let service = FileSystemService::new(fs.clone());

    let opened = service.open_document(&path).expect("readable");
    let DocumentContent::Text { text, crlf, .. } = opened.content else {
        panic!("expected editable text");
    };
    // CRLF is normalised for editing…
    assert_eq!(text, "int a;\nint b;\n");
    assert!(crlf);

    // …and restored byte-for-byte on the way back out.
    let stamp = service.save_document(&path, &text, crlf).expect("writable");
    assert_eq!(
        fs.read_file(&path).unwrap(),
        b"int a;\r\nint b;\r\n".to_vec()
    );
    // The post-write mtime is handed back so the caller can ignore its own
    // watcher event.
    assert!(stamp.is_some());
    assert_ne!(stamp, opened.modified);
}

#[test]
fn open_document_reports_binary_and_missing_files() {
    use crate::domain::text_document::DocumentContent;

    let path = PathBuf::from("/proj/build/libx.dylib");
    let fs = Rc::new(FakeFs::with_file(&path, &[0x00, 0xFF, 0xFE]));
    let service = FileSystemService::new(fs);

    assert_eq!(
        service.open_document(&path).unwrap().content,
        DocumentContent::Binary
    );
    assert!(matches!(
        service.open_document(Path::new("/proj/gone.cpp")),
        Err(crate::domain::file_system::FileSystemError::NotFound(_))
    ));
}

#[test]
fn dirty_tabs_survive_deletion_so_the_buffer_can_be_saved_back() {
    let path = PathBuf::from("/proj/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"int a;\n"));
    let mut state = state_over(fs.clone());

    open_tab(&mut state, &path);
    assert_eq!(state.tabs.len(), 1);

    fs.files.borrow_mut().remove(&path);
    prune_missing(&mut state);
    assert!(state.tabs.is_empty());

    fs.write_file(&path, b"int a;\n").unwrap();
    open_tab(&mut state, &path);
    edit_active(&mut state, "int a; // edited\n");
    assert!(state.tabs[0].unsaved());

    fs.files.borrow_mut().remove(&path);
    prune_missing(&mut state);
    assert_eq!(state.tabs.len(), 1, "a dirty buffer must not be discarded");

    assert!(save_tab(&mut state, 0));
    assert_eq!(fs.read_file(&path).unwrap(), b"int a; // edited\n".to_vec());
    assert!(!state.tabs[0].unsaved());
}

#[test]
fn renaming_a_parent_folder_keeps_the_unsaved_buffer() {
    let path = PathBuf::from("/proj/src/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"original\n"));
    let mut state = state_over(fs);

    open_tab(&mut state, &path);
    edit_active(&mut state, "typed but not saved\n");

    retarget_after_rename(
        &mut state,
        Path::new("/proj/src"),
        Path::new("/proj/source"),
    );

    let tab = &state.tabs[0];
    assert_eq!(tab.path, PathBuf::from("/proj/source/main.cpp"));
    assert_eq!(tab.name, "main.cpp");
    assert_eq!(
        tab.content.text(),
        Some("typed but not saved\n"),
        "the rename must not re-read the file over the user's edits"
    );
    assert!(tab.unsaved());
}

#[test]
fn external_changes_reload_clean_buffers_and_flag_dirty_ones() {
    let clean = PathBuf::from("/proj/clean.cpp");
    let dirty = PathBuf::from("/proj/dirty.cpp");
    let fs = Rc::new(FakeFs::default());
    fs.write_file(&clean, b"one\n").unwrap();
    fs.write_file(&dirty, b"one\n").unwrap();
    let mut state = state_over(fs.clone());

    open_tab(&mut state, &clean);
    open_tab(&mut state, &dirty);
    state.active_tab = 1;
    edit_active(&mut state, "my unsaved work\n");

    fs.write_file(&clean, b"two\n").unwrap();
    fs.write_file(&dirty, b"two\n").unwrap();
    sync_open_buffers(&mut state, &[clean.clone(), dirty.clone()]);

    assert_eq!(state.tabs[0].content.text(), Some("two\n"));
    assert!(!state.tabs[0].external_change);
    assert!(
        state.tabs[0].history_reset,
        "a replaced buffer must drop its undo history, or ⌘Z rewinds past the reload"
    );

    assert_eq!(
        state.tabs[1].content.text(),
        Some("my unsaved work\n"),
        "a dirty buffer must never be overwritten by the watcher"
    );
    assert!(state.tabs[1].external_change);
    assert!(
        !state.tabs[1].history_reset,
        "nothing was replaced, so this buffer keeps its history"
    );
}

#[test]
fn our_own_save_is_not_mistaken_for_an_external_change() {
    let path = PathBuf::from("/proj/main.cpp");
    let fs = Rc::new(FakeFs::with_file(&path, b"one\n"));
    let mut state = state_over(fs);

    open_tab(&mut state, &path);
    edit_active(&mut state, "two\n");
    assert!(save_tab(&mut state, 0));

    sync_open_buffers(&mut state, &[path]);
    assert!(!state.tabs[0].external_change);
    assert!(!state.tabs[0].unsaved());
}

#[test]
fn binary_files_open_read_only_and_are_never_saved() {
    let path = PathBuf::from("/proj/build/libx.dylib");
    let fs = Rc::new(FakeFs::with_file(&path, &[0x00, 0xFF]));
    let mut state = state_over(fs.clone());

    open_tab(&mut state, &path);
    assert_eq!(state.tabs[0].content, DocumentContent::Binary);
    assert!(!state.tabs[0].unsaved());

    assert!(!save_tab(&mut state, 0));
    assert_eq!(fs.read_file(&path).unwrap(), vec![0x00, 0xFF]);
}
