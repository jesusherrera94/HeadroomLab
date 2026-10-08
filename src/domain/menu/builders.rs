//! Builds each top-level menu from a `MenuContext`, deciding which entries
//! exist and which are enabled.

use super::chord::*;
use super::{
    Menu, MenuCommand, MenuContext, MenuEntry, MenuItem, MenuSurface, PredefinedItem,
    TransportCommand, WindowId,
};
use crate::domain::editing::EditorCommand;

fn item(label: impl Into<String>, command: MenuCommand, enabled: bool) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.into(),
        shortcut: None,
        command,
        enabled,
        checked: None,
    })
}

fn keyed(label: impl Into<String>, chord: Chord, command: MenuCommand, enabled: bool) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.into(),
        shortcut: Some(chord),
        command,
        enabled,
        checked: None,
    })
}

fn checkable(
    label: impl Into<String>,
    command: MenuCommand,
    enabled: bool,
    checked: bool,
) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.into(),
        shortcut: None,
        command,
        enabled,
        checked: Some(checked),
    })
}

fn predefined(label: &str, chord: Option<Chord>, which: PredefinedItem) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        label: label.to_string(),
        shortcut: chord,
        command: MenuCommand::Predefined(which),
        enabled: true,
        checked: None,
    })
}

pub(super) fn app_menu(ctx: &MenuContext) -> Menu {
    Menu {
        title: "HeadroomLab".to_string(),
        entries: vec![
            item("About HeadroomLab", MenuCommand::About, true),
            item(
                "Check for Updates…",
                MenuCommand::CheckForUpdates,
                ctx.updates_available,
            ),
            MenuEntry::Separator,
            predefined("Services", None, PredefinedItem::Services),
            predefined("Hide HeadroomLab", Some(HIDE), PredefinedItem::Hide),
            predefined("Hide Others", Some(HIDE_OTHERS), PredefinedItem::HideOthers),
            predefined("Show All", None, PredefinedItem::ShowAll),
            MenuEntry::Separator,
            keyed("Quit HeadroomLab", QUIT, MenuCommand::Quit, true),
        ],
    }
}

fn file_active(ctx: &MenuContext) -> bool {
    matches!(ctx.focused, WindowId::Initial | WindowId::Editor)
}

pub(super) fn file_menu(ctx: &MenuContext, surface: MenuSurface) -> Menu {
    let active = file_active(ctx);
    let editing = active && ctx.has_project;

    let mut entries = vec![
        keyed("New Project…", NEW_PROJECT, MenuCommand::NewProject, active),
        keyed(
            "Open Project…",
            OPEN_PROJECT,
            MenuCommand::OpenProject,
            active,
        ),
        MenuEntry::Submenu(recents_menu(ctx, active)),
        MenuEntry::Separator,
        keyed("New File", NEW_FILE, MenuCommand::NewFile, editing),
        keyed("New Folder", NEW_FOLDER, MenuCommand::NewFolder, editing),
        MenuEntry::Separator,
        keyed(
            "Save",
            SAVE,
            MenuCommand::Save,
            active && ctx.active_tab_dirty,
        ),
        keyed(
            "Save All",
            SAVE_ALL,
            MenuCommand::SaveAll,
            active && ctx.any_tab_dirty,
        ),
        MenuEntry::Separator,
        keyed(
            "Close Tab",
            CLOSE_TAB,
            MenuCommand::CloseTab,
            ctx.focused == WindowId::Editor && ctx.has_open_tab,
        ),
        keyed(
            "Close Window",
            if ctx.focused == WindowId::Editor {
                CLOSE_WINDOW
            } else {
                CLOSE_TAB
            },
            MenuCommand::CloseWindow,
            true,
        ),
    ];

    if matches!(surface, MenuSurface::Window(_)) {
        entries.push(MenuEntry::Separator);
        entries.push(item("About HeadroomLab", MenuCommand::About, true));
        entries.push(keyed("Quit HeadroomLab", QUIT, MenuCommand::Quit, true));
    }

    Menu {
        title: "File".to_string(),
        entries,
    }
}

fn recents_menu(ctx: &MenuContext, active: bool) -> Menu {
    let mut entries: Vec<MenuEntry> = ctx
        .recents
        .iter()
        .enumerate()
        .map(|(index, label)| item(label.clone(), MenuCommand::OpenRecent(index), active))
        .collect();

    if !entries.is_empty() {
        entries.push(MenuEntry::Separator);
        entries.push(item("Clear Menu", MenuCommand::ClearRecents, active));
    }

    Menu {
        title: "Open Recent".to_string(),
        entries,
    }
}

pub(super) fn edit_menu(ctx: &MenuContext) -> Menu {
    let base = ctx.focused == WindowId::Editor && ctx.has_open_tab && !ctx.other_widget_focused;
    let writable = base && ctx.active_tab_editable;

    let edit = |label: &str, command: EditorCommand, enabled: bool| {
        keyed(
            label,
            chord_for(command),
            MenuCommand::Edit(command),
            enabled,
        )
    };

    Menu {
        title: "Edit".to_string(),
        entries: vec![
            edit("Undo", EditorCommand::Undo, writable),
            edit("Redo", EditorCommand::Redo, writable),
            MenuEntry::Separator,
            edit("Cut", EditorCommand::Cut, writable && ctx.has_selection),
            edit("Copy", EditorCommand::Copy, base && ctx.has_selection),
            edit("Paste", EditorCommand::Paste, writable && ctx.can_paste),
            edit("Select All", EditorCommand::SelectAll, base),
            MenuEntry::Separator,
            edit("Select Line", EditorCommand::SelectLine, base),
            edit(
                "Select Next Occurrence",
                EditorCommand::SelectNextOccurrence,
                base,
            ),
            edit("Duplicate Line", EditorCommand::DuplicateLine, writable),
            edit("Delete Line", EditorCommand::DeleteLine, writable),
            edit(
                "Toggle Comment",
                EditorCommand::ToggleComment,
                writable && ctx.can_comment,
            ),
            MenuEntry::Separator,
            keyed("Find…", FIND, MenuCommand::Find, writable),
        ],
    }
}

pub(super) fn build_menu(ctx: &MenuContext) -> Menu {
    let active = ctx.focused == WindowId::Editor && ctx.has_project;
    let can_build = active && !ctx.build_running;

    Menu {
        title: "Build".to_string(),
        entries: vec![
            item("Open Emulator", MenuCommand::OpenEmulator, active),
            keyed(
                "Build & Run Emulator",
                BUILD_RUN,
                MenuCommand::BuildRun,
                can_build,
            ),
            keyed("Compile", COMPILE, MenuCommand::Compile, can_build),
        ],
    }
}

pub(super) fn transport_menu(ctx: &MenuContext) -> Menu {
    let active = ctx.focused == WindowId::Simulator;
    let loaded = active && ctx.has_audio;

    Menu {
        title: "Transport".to_string(),
        entries: vec![
            item(
                "Load Audio File…",
                MenuCommand::Transport(TransportCommand::LoadAudio),
                active,
            ),
            keyed(
                if ctx.is_playing { "Pause" } else { "Play" },
                PLAY_PAUSE,
                MenuCommand::Transport(TransportCommand::PlayPause),
                loaded,
            ),
            checkable(
                "Bypass Effect",
                MenuCommand::Transport(TransportCommand::Bypass),
                loaded,
                ctx.is_bypassed,
            ),
            MenuEntry::Separator,
            item(
                "View Signal Graph",
                MenuCommand::Transport(TransportCommand::ViewGraph),
                loaded,
            ),
        ],
    }
}

pub(super) fn window_menu(ctx: &MenuContext) -> Menu {
    let mut entries = Vec::new();

    let mut window = |id: WindowId, enabled: bool| {
        entries.push(checkable(
            id.menu_label(),
            MenuCommand::FocusWindow(id),
            enabled,
            ctx.focused == id,
        ));
    };

    window(WindowId::Editor, ctx.has_project);
    window(WindowId::Simulator, ctx.has_project);
    window(WindowId::Graph, ctx.graph_open || ctx.has_audio);
    if ctx.doom_open {
        window(WindowId::Doom, true);
    }

    Menu {
        title: "Window".to_string(),
        entries,
    }
}

pub(super) fn help_menu(ctx: &MenuContext, surface: MenuSurface) -> Menu {
    let mut entries = vec![item("HeadroomLab Help", MenuCommand::Help, true)];
    if surface != MenuSurface::Global {
        entries.push(MenuEntry::Separator);
        entries.push(item(
            "Check for Updates…",
            MenuCommand::CheckForUpdates,
            ctx.updates_available,
        ));
    }

    Menu {
        title: "Help".to_string(),
        entries,
    }
}
