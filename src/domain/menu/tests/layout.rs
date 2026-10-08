use super::*;

#[test]
fn doom_is_absent_from_the_window_menu_until_it_is_open() {
    let hidden = global(&editing());
    assert!(
        hidden
            .find(MenuCommand::FocusWindow(WindowId::Doom))
            .is_none()
    );

    let shown = global(&MenuContext {
        doom_open: true,
        ..editing()
    });
    assert!(shown.is_enabled(MenuCommand::FocusWindow(WindowId::Doom)));
}

#[test]
fn the_window_menu_offers_no_way_to_minimise() {
    let model = global(&editing());
    let window = model.menus.iter().find(|m| m.title == "Window").unwrap();
    for entry in &window.entries {
        if let MenuEntry::Item(item) = entry {
            assert!(
                !matches!(item.command, MenuCommand::Predefined(_)),
                "{:?} is an OS-handled item the app cannot undo",
                item.label
            );
        }
    }
}

#[test]
fn the_focused_window_is_ticked() {
    let model = global(&editing());
    let editor = model
        .find(MenuCommand::FocusWindow(WindowId::Editor))
        .unwrap();
    let simulator = model
        .find(MenuCommand::FocusWindow(WindowId::Simulator))
        .unwrap();
    assert_eq!(editor.checked, Some(true));
    assert_eq!(simulator.checked, Some(false));
}

#[test]
fn the_graph_waits_for_audio_unless_it_is_already_open() {
    let quiet = global(&editing());
    assert!(!quiet.is_enabled(MenuCommand::FocusWindow(WindowId::Graph)));

    let loaded = global(&MenuContext {
        has_audio: true,
        ..editing()
    });
    assert!(loaded.is_enabled(MenuCommand::FocusWindow(WindowId::Graph)));

    let already = global(&MenuContext {
        graph_open: true,
        ..editing()
    });
    assert!(already.is_enabled(MenuCommand::FocusWindow(WindowId::Graph)));
}

/// D4: the one bar macOS shares between every window must never reorder or
/// drop a title as focus moves — only enablement may change.
#[test]
fn global_menu_titles_never_change() {
    let expected = vec![
        "HeadroomLab",
        "File",
        "Edit",
        "Build",
        "Transport",
        "Window",
        "Help",
    ];
    for focused in [
        WindowId::Splash,
        WindowId::Initial,
        WindowId::Editor,
        WindowId::Simulator,
        WindowId::Graph,
        WindowId::Doom,
    ] {
        for has_project in [false, true] {
            let model = global(&MenuContext {
                focused,
                has_project,
                ..MenuContext::default()
            });
            assert_eq!(model.titles(), expected, "titles moved for {focused:?}");
        }
    }
}

#[test]
fn per_window_titles_match_the_spec() {
    let ctx = editing();
    let titles = |window| {
        MenuModel::build(&ctx, MenuSurface::Window(window))
            .titles()
            .join(" ")
    };

    assert_eq!(titles(WindowId::Editor), "File Edit Build Window Help");
    assert_eq!(titles(WindowId::Simulator), "File Transport Window Help");
    assert_eq!(titles(WindowId::Graph), "File Window Help");
    assert_eq!(titles(WindowId::Splash), "");
    assert_eq!(titles(WindowId::Initial), "");
    assert_eq!(titles(WindowId::Doom), "");
}

#[test]
fn the_two_surfaces_agree_on_enablement() {
    let ctx = MenuContext {
        has_selection: true,
        can_paste: true,
        any_tab_dirty: true,
        active_tab_dirty: true,
        ..editing()
    };
    let global = MenuModel::build(&ctx, MenuSurface::Global);
    let per_window = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Editor));

    for command in [
        MenuCommand::NewProject,
        MenuCommand::OpenProject,
        MenuCommand::NewFile,
        MenuCommand::NewFolder,
        MenuCommand::Save,
        MenuCommand::SaveAll,
        MenuCommand::CloseTab,
        MenuCommand::CloseWindow,
        MenuCommand::Find,
        MenuCommand::BuildRun,
        MenuCommand::Compile,
        MenuCommand::OpenEmulator,
        MenuCommand::Quit,
        MenuCommand::About,
        MenuCommand::Help,
        MenuCommand::Edit(EditorCommand::Cut),
        MenuCommand::Edit(EditorCommand::Paste),
        MenuCommand::FocusWindow(WindowId::Simulator),
    ] {
        assert_eq!(
            global.is_enabled(command),
            per_window.is_enabled(command),
            "{command:?} differs between surfaces"
        );
    }
}

#[test]
fn a_per_window_bar_assumes_its_own_window_is_focused() {
    let ctx = MenuContext {
        focused: WindowId::Editor,
        has_audio: true,
        ..editing()
    };
    let simulator = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Simulator));
    assert!(simulator.is_enabled(MenuCommand::Transport(TransportCommand::PlayPause)));
}

#[test]
fn check_for_updates_sits_where_the_platform_expects_it() {
    let ctx = MenuContext {
        updates_available: true,
        ..editing()
    };

    let global = MenuModel::build(&ctx, MenuSurface::Global);
    let app_menu = global
        .menus
        .iter()
        .find(|m| m.title == "HeadroomLab")
        .unwrap();
    assert!(
        app_menu
            .entries
            .iter()
            .any(|e| matches!(e, MenuEntry::Item(i)
                if i.command == MenuCommand::CheckForUpdates)),
        "macOS wants it beside About"
    );
    let global_help = global.menus.iter().find(|m| m.title == "Help").unwrap();
    assert!(
        !global_help
            .entries
            .iter()
            .any(|e| matches!(e, MenuEntry::Item(i)
                if i.command == MenuCommand::CheckForUpdates)),
        "and not in Help as well"
    );

    let per_window = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Editor));
    let help = per_window.menus.iter().find(|m| m.title == "Help").unwrap();
    assert!(
        help.entries.iter().any(|e| matches!(e, MenuEntry::Item(i)
                if i.command == MenuCommand::CheckForUpdates)),
        "off macOS there is no app menu, so Help is its home"
    );
}

#[test]
fn the_app_menu_is_folded_into_file_off_macos() {
    let ctx = editing();
    let per_window = MenuModel::build(&ctx, MenuSurface::Window(WindowId::Editor));
    assert!(!per_window.titles().contains(&"HeadroomLab"));
    assert!(per_window.is_enabled(MenuCommand::About));
    assert!(per_window.is_enabled(MenuCommand::Quit));
}

#[test]
fn chords_render_for_the_host_platform() {
    if cfg!(target_os = "macos") {
        assert_eq!(SAVE.to_string(), "⌘S");
        assert_eq!(SAVE_ALL.to_string(), "⇧⌘S");
        assert_eq!(NEW_FOLDER.to_string(), "⇧⌥⌘N");
        assert_eq!(DUPLICATE_LINE.to_string(), "⇧⌥↓");
        assert_eq!(PLAY_PAUSE.to_string(), "Space");
    } else {
        assert_eq!(SAVE.to_string(), "Ctrl+S");
        assert_eq!(SAVE_ALL.to_string(), "Ctrl+Shift+S");
        assert_eq!(NEW_FOLDER.to_string(), "Ctrl+Shift+Alt+N");
        assert_eq!(DUPLICATE_LINE.to_string(), "Shift+Alt+Down");
        assert_eq!(PLAY_PAUSE.to_string(), "Space");
    }
}
