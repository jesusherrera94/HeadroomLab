use super::*;

#[test]
fn save_follows_the_dirty_flags() {
    let clean = global(&editing());
    assert!(!clean.is_enabled(MenuCommand::Save));
    assert!(!clean.is_enabled(MenuCommand::SaveAll));

    let dirty = global(&MenuContext {
        active_tab_dirty: true,
        any_tab_dirty: true,
        ..editing()
    });
    assert!(dirty.is_enabled(MenuCommand::Save));
    assert!(dirty.is_enabled(MenuCommand::SaveAll));
}

#[test]
fn save_all_can_be_enabled_while_save_is_not() {
    let model = global(&MenuContext {
        any_tab_dirty: true,
        ..editing()
    });
    assert!(!model.is_enabled(MenuCommand::Save));
    assert!(model.is_enabled(MenuCommand::SaveAll));
}

#[test]
fn edit_commands_are_disabled_unless_the_editor_is_focused() {
    for focused in [
        WindowId::Splash,
        WindowId::Initial,
        WindowId::Simulator,
        WindowId::Graph,
        WindowId::Doom,
    ] {
        let model = global(&MenuContext {
            focused,
            has_selection: true,
            can_paste: true,
            ..editing()
        });
        for command in [
            EditorCommand::Undo,
            EditorCommand::Redo,
            EditorCommand::Cut,
            EditorCommand::Copy,
            EditorCommand::Paste,
            EditorCommand::SelectAll,
            EditorCommand::SelectLine,
            EditorCommand::SelectNextOccurrence,
            EditorCommand::ToggleComment,
            EditorCommand::DuplicateLine,
            EditorCommand::DeleteLine,
        ] {
            assert!(
                !model.is_enabled(MenuCommand::Edit(command)),
                "{command:?} enabled while {focused:?} had focus"
            );
        }
        assert!(!model.is_enabled(MenuCommand::Find));
    }
}

#[test]
fn doom_greys_out_everything_but_quit_and_close_window() {
    let model = global(&MenuContext {
        focused: WindowId::Doom,
        doom_open: true,
        any_tab_dirty: true,
        active_tab_dirty: true,
        ..editing()
    });

    assert!(!model.is_enabled(MenuCommand::NewProject));
    assert!(!model.is_enabled(MenuCommand::OpenProject));
    assert!(!model.is_enabled(MenuCommand::NewFile));
    assert!(!model.is_enabled(MenuCommand::Save));
    assert!(!model.is_enabled(MenuCommand::SaveAll));
    assert!(!model.is_enabled(MenuCommand::CloseTab));
    assert!(!model.is_enabled(MenuCommand::BuildRun));
    assert!(!model.is_enabled(MenuCommand::Compile));
    assert!(!model.is_enabled(MenuCommand::OpenEmulator));

    assert!(model.is_enabled(MenuCommand::Quit));
    assert!(model.is_enabled(MenuCommand::CloseWindow));
}

#[test]
fn splash_greys_out_the_file_menu() {
    let model = global(&MenuContext::default());
    assert!(!model.is_enabled(MenuCommand::NewProject));
    assert!(!model.is_enabled(MenuCommand::OpenProject));
    assert!(model.is_enabled(MenuCommand::Quit));
    assert!(model.is_enabled(MenuCommand::CloseWindow));
}

#[test]
fn cut_and_copy_need_a_selection_and_paste_needs_a_clipboard() {
    let bare = global(&editing());
    assert!(!bare.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
    assert!(!bare.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
    assert!(!bare.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));

    let ready = global(&MenuContext {
        has_selection: true,
        can_paste: true,
        ..editing()
    });
    assert!(ready.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
    assert!(ready.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
    assert!(ready.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
}

#[test]
fn a_read_only_buffer_can_be_copied_but_not_rewritten() {
    let model = global(&MenuContext {
        active_tab_editable: false,
        has_selection: true,
        can_paste: true,
        ..editing()
    });
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::SelectAll)));
    assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
    assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
    assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::DeleteLine)));
}

#[test]
fn the_edit_menu_stands_down_while_another_widget_has_focus() {
    let taken = global(&MenuContext {
        other_widget_focused: true,
        has_selection: true,
        can_paste: true,
        ..editing()
    });
    assert!(!taken.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
    assert!(!taken.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
    assert!(!taken.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
    assert!(!taken.is_enabled(MenuCommand::Edit(EditorCommand::SelectAll)));
    assert!(!taken.is_enabled(MenuCommand::Find));
    let dirty = global(&MenuContext {
        other_widget_focused: true,
        active_tab_dirty: true,
        ..editing()
    });
    assert!(dirty.is_enabled(MenuCommand::Save));
}

#[test]
fn the_edit_menu_is_live_when_nothing_else_holds_focus() {
    let model = global(&MenuContext {
        other_widget_focused: false,
        has_selection: true,
        can_paste: true,
        ..editing()
    });
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::Copy)));
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::Cut)));
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::Paste)));
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::SelectAll)));
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::Undo)));
    assert!(model.is_enabled(MenuCommand::Edit(EditorCommand::ToggleComment)));
    assert!(model.is_enabled(MenuCommand::Find));
}

#[test]
fn close_window_takes_cmd_w_wherever_there_are_no_tabs() {
    let editor = global(&editing());
    assert_eq!(
        editor.find(MenuCommand::CloseTab).unwrap().shortcut,
        Some(CLOSE_TAB)
    );
    assert_eq!(
        editor.find(MenuCommand::CloseWindow).unwrap().shortcut,
        Some(CLOSE_WINDOW)
    );

    for focused in [WindowId::Simulator, WindowId::Graph, WindowId::Doom] {
        let model = global(&MenuContext {
            focused,
            ..editing()
        });
        let close_tab = model.find(MenuCommand::CloseTab).unwrap();
        let close_window = model.find(MenuCommand::CloseWindow).unwrap();
        assert!(!close_tab.enabled, "{focused:?} has no tabs to close");
        assert_eq!(close_window.shortcut, Some(CLOSE_TAB));
        assert!(close_window.enabled);
    }
}

#[test]
fn toggle_comment_needs_a_language_with_line_comments() {
    let model = global(&MenuContext {
        can_comment: false,
        ..editing()
    });
    assert!(!model.is_enabled(MenuCommand::Edit(EditorCommand::ToggleComment)));
}

#[test]
fn open_recent_is_empty_without_recents_and_capped_by_the_caller() {
    let none = global(&editing());
    assert!(none.find(MenuCommand::OpenRecent(0)).is_none());
    assert!(none.find(MenuCommand::ClearRecents).is_none());

    let ctx = MenuContext {
        recents: vec!["A".into(), "B".into(), "C".into()],
        ..editing()
    };
    let some = global(&ctx);
    assert!(some.is_enabled(MenuCommand::OpenRecent(2)));
    assert!(some.find(MenuCommand::OpenRecent(3)).is_none());
    assert!(some.is_enabled(MenuCommand::ClearRecents));
}

#[test]
fn a_running_build_disables_both_build_actions() {
    let model = global(&MenuContext {
        build_running: true,
        ..editing()
    });
    assert!(!model.is_enabled(MenuCommand::BuildRun));
    assert!(!model.is_enabled(MenuCommand::Compile));
    // Opening the emulator on the previously built library is still fine.
    assert!(model.is_enabled(MenuCommand::OpenEmulator));
}

#[test]
fn transport_needs_the_simulator_focused_and_audio_loaded() {
    let unfocused = global(&MenuContext {
        has_audio: true,
        ..editing()
    });
    assert!(!unfocused.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));

    let silent = global(&MenuContext {
        focused: WindowId::Simulator,
        ..editing()
    });
    assert!(silent.is_enabled(MenuCommand::Transport(TransportCommand::LoadAudio)));
    assert!(!silent.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));

    let loaded = global(&MenuContext {
        focused: WindowId::Simulator,
        has_audio: true,
        ..editing()
    });
    assert!(loaded.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));
    assert!(loaded.is_enabled(MenuCommand::Transport(TransportCommand::ViewGraph)));
}

#[test]
fn play_pause_follows_the_playing_flag_and_bypass_is_checkable() {
    let ctx = MenuContext {
        focused: WindowId::Simulator,
        has_audio: true,
        ..editing()
    };
    let stopped = global(&ctx);
    assert_eq!(
        stopped
            .find(MenuCommand::Transport(TransportCommand::PlayPause))
            .unwrap()
            .label,
        "Play"
    );

    let playing = global(&MenuContext {
        is_playing: true,
        is_bypassed: true,
        ..ctx
    });
    let item = playing
        .find(MenuCommand::Transport(TransportCommand::PlayPause))
        .unwrap();
    assert_eq!(item.label, "Pause");
    assert_eq!(
        playing
            .find(MenuCommand::Transport(TransportCommand::Bypass))
            .unwrap()
            .checked,
        Some(true)
    );
}

#[test]
fn check_for_updates_greys_out_when_updates_are_disabled() {
    let off = global(&editing());
    assert!(off.find(MenuCommand::CheckForUpdates).is_some());
    assert!(!off.is_enabled(MenuCommand::CheckForUpdates));

    let on = global(&MenuContext {
        updates_available: true,
        ..editing()
    });
    assert!(on.is_enabled(MenuCommand::CheckForUpdates));
}
