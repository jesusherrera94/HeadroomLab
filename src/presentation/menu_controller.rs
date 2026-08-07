//! Turns a [`MenuCommand`] into effects.
//!
//! Commands that act on the open project are applied here, straight onto
//! [`EditorState`] through the editor controller's public API. Everything that
//! opens, closes or focuses a window is *returned* in [`MenuRequests`] instead:
//! creating a viewport is only safe at one point in the frame (see
//! `app_controller::prepare_simulator`), and that point is the app controller's
//! to choose.
//!
//! Edit commands are not executed here at all — they are queued for the code
//! area, which runs them through the same path a key chord takes.

use eframe::egui;

use crate::domain::menu::{Chord, MenuCommand, TransportCommand, WindowId};
use crate::presentation::editor_controller::{self, EditorState, EntryKind};

/// What the menu asked the app controller to do this frame.
#[derive(Default)]
pub struct MenuRequests {
    /// Show the Initial window as a sibling. `true` also opens its Create modal.
    pub open_initial: Option<OpenInitial>,
    /// Open the recent project at this index.
    pub open_recent: Option<usize>,
    pub clear_recents: bool,
    /// Quit, via the unsaved-work guard.
    pub quit: bool,
    /// Close the focused window. In the Editor this is a quit.
    pub close_window: bool,
    /// Focus that window, opening it first if it is closed.
    pub focus_window: Option<WindowId>,
    pub show_about: bool,
    pub open_help: bool,
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    pub transport: Option<TransportCommand>,
}

/// Which way the Initial window was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenInitial {
    /// File ▸ Open Project… — the picker as it normally opens.
    Picker,
    /// File ▸ New Project… — with the Create modal already up.
    CreateModal,
}

/// Applies `command`. `editor` is `None` before a project is open, in which case
/// every editor-scoped command is a no-op — the menu greys those out, so this is
/// belt-and-braces against a stale click.
pub fn dispatch(command: MenuCommand, editor: Option<&mut EditorState>) -> MenuRequests {
    let mut requests = MenuRequests::default();

    match command {
        // -- Window-level: the app controller's business --------------------
        MenuCommand::NewProject => requests.open_initial = Some(OpenInitial::CreateModal),
        MenuCommand::OpenProject => requests.open_initial = Some(OpenInitial::Picker),
        MenuCommand::OpenRecent(index) => requests.open_recent = Some(index),
        MenuCommand::ClearRecents => requests.clear_recents = true,
        MenuCommand::Quit => requests.quit = true,
        MenuCommand::CloseWindow => requests.close_window = true,
        MenuCommand::FocusWindow(window) => requests.focus_window = Some(window),
        MenuCommand::About => requests.show_about = true,
        MenuCommand::Help => requests.open_help = true,
        MenuCommand::OpenEmulator => requests.open_emulator = true,
        MenuCommand::BuildRun => requests.build_run = true,
        MenuCommand::Compile => requests.compile = true,
        MenuCommand::Transport(action) => requests.transport = Some(action),

        // -- Editor-scoped: applied here ------------------------------------
        MenuCommand::NewFile => {
            if let Some(state) = editor {
                editor_controller::begin_create_at_selection(state, EntryKind::File);
            }
        }
        MenuCommand::NewFolder => {
            if let Some(state) = editor {
                editor_controller::begin_create_at_selection(state, EntryKind::Directory);
            }
        }
        MenuCommand::Save => {
            if let Some(state) = editor {
                editor_controller::save_active_tab(state);
            }
        }
        MenuCommand::SaveAll => {
            if let Some(state) = editor {
                editor_controller::save_all_tabs(state);
            }
        }
        MenuCommand::CloseTab => {
            if let Some(state) = editor {
                editor_controller::close_active_tab(state);
            }
        }
        MenuCommand::Find => {
            if let Some(state) = editor {
                editor_controller::open_find(state);
            }
        }
        // Never run here: the code area owns every edit, and running one from
        // this side would be the second implementation this design exists to
        // avoid.
        MenuCommand::Edit(command) => {
            if let Some(state) = editor {
                editor_controller::queue_command(state, command);
            }
        }

        // Handed to the OS by the native adapter; never reaches this function.
        MenuCommand::Predefined(_) => {}
    }

    requests
}

/// The egui shortcut a [`Chord`] stands for, so the chord a menu *shows* and the
/// chord that actually fires come from one table (AC5).
///
/// `Modifiers::COMMAND` is ⌘ on macOS and Ctrl elsewhere — the same mapping
/// [`Chord`]'s own `Display` uses.
pub fn shortcut(chord: Chord) -> egui::KeyboardShortcut {
    let mut modifiers = egui::Modifiers::NONE;
    if chord.command {
        modifiers = modifiers.plus(egui::Modifiers::COMMAND);
    }
    if chord.shift {
        modifiers = modifiers.plus(egui::Modifiers::SHIFT);
    }
    if chord.alt {
        modifiers = modifiers.plus(egui::Modifiers::ALT);
    }
    egui::KeyboardShortcut::new(modifiers, key_for(chord.key))
}

/// Maps a chord's key spelling onto egui's key. Panics in debug on an unknown
/// spelling: the table is a fixed set of constants, so a miss is a typo in this
/// file rather than anything a user can provoke.
fn key_for(key: &str) -> egui::Key {
    match key {
        "A" => egui::Key::A,
        "B" => egui::Key::B,
        "C" => egui::Key::C,
        "D" => egui::Key::D,
        "F" => egui::Key::F,
        "H" => egui::Key::H,
        "K" => egui::Key::K,
        "L" => egui::Key::L,
        "M" => egui::Key::M,
        "N" => egui::Key::N,
        "O" => egui::Key::O,
        "Q" => egui::Key::Q,
        "R" => egui::Key::R,
        "S" => egui::Key::S,
        "V" => egui::Key::V,
        "W" => egui::Key::W,
        "X" => egui::Key::X,
        "Z" => egui::Key::Z,
        "/" => egui::Key::Slash,
        "Down" => egui::Key::ArrowDown,
        "Up" => egui::Key::ArrowUp,
        "Space" => egui::Key::Space,
        other => {
            debug_assert!(false, "no egui::Key for chord key {other:?}");
            egui::Key::Escape
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::editing::EditorCommand;
    use crate::domain::menu;

    #[test]
    fn window_commands_are_returned_rather_than_applied() {
        assert_eq!(
            dispatch(MenuCommand::NewProject, None).open_initial,
            Some(OpenInitial::CreateModal)
        );
        assert_eq!(
            dispatch(MenuCommand::OpenProject, None).open_initial,
            Some(OpenInitial::Picker)
        );
        assert_eq!(
            dispatch(MenuCommand::OpenRecent(2), None).open_recent,
            Some(2)
        );
        assert!(dispatch(MenuCommand::Quit, None).quit);
        assert!(dispatch(MenuCommand::CloseWindow, None).close_window);
        assert!(dispatch(MenuCommand::Compile, None).compile);
        assert!(dispatch(MenuCommand::BuildRun, None).build_run);
        assert!(dispatch(MenuCommand::OpenEmulator, None).open_emulator);
        assert!(dispatch(MenuCommand::Help, None).open_help);
        assert!(dispatch(MenuCommand::About, None).show_about);
        assert_eq!(
            dispatch(MenuCommand::FocusWindow(WindowId::Graph), None).focus_window,
            Some(WindowId::Graph)
        );
        assert_eq!(
            dispatch(MenuCommand::Transport(TransportCommand::PlayPause), None).transport,
            Some(TransportCommand::PlayPause)
        );
    }

    /// Editor-scoped commands ask nothing of the app controller — they act on
    /// the state directly, so every request field stays clear.
    #[test]
    fn editor_commands_raise_no_window_requests() {
        for command in [
            MenuCommand::Save,
            MenuCommand::SaveAll,
            MenuCommand::NewFile,
            MenuCommand::NewFolder,
            MenuCommand::CloseTab,
            MenuCommand::Find,
            MenuCommand::Edit(EditorCommand::Undo),
        ] {
            let requests = dispatch(command, None);
            assert!(requests.open_initial.is_none());
            assert!(!requests.quit);
            assert!(!requests.close_window);
            assert!(requests.focus_window.is_none());
        }
    }

    #[test]
    fn predefined_items_do_nothing_here() {
        let requests = dispatch(MenuCommand::Predefined(menu::PredefinedItem::Hide), None);
        assert!(!requests.quit);
        assert!(!requests.close_window);
        assert!(requests.focus_window.is_none());
    }

    /// The chord a menu shows and the chord that fires must be the same one.
    #[test]
    fn chords_convert_to_the_expected_egui_shortcut() {
        assert_eq!(
            shortcut(menu::SAVE),
            egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S)
        );
        assert_eq!(
            shortcut(menu::SAVE_ALL),
            egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                egui::Key::S
            )
        );
        assert_eq!(
            shortcut(menu::FIND),
            egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F)
        );
        assert_eq!(
            shortcut(menu::CLOSE_TAB),
            egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::W)
        );
        assert_eq!(
            shortcut(menu::PLAY_PAUSE),
            egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Space)
        );
    }

    /// Every chord in the table must have an egui key — `key_for` debug-asserts,
    /// so a missing arm fails right here rather than at the first keypress.
    #[test]
    fn every_chord_in_the_table_maps_to_a_key() {
        for chord in [
            menu::SAVE,
            menu::SAVE_ALL,
            menu::FIND,
            menu::NEW_PROJECT,
            menu::OPEN_PROJECT,
            menu::NEW_FILE,
            menu::NEW_FOLDER,
            menu::CLOSE_TAB,
            menu::CLOSE_WINDOW,
            menu::QUIT,
            menu::HIDE,
            menu::HIDE_OTHERS,
            menu::BUILD_RUN,
            menu::COMPILE,
            menu::PLAY_PAUSE,
            menu::UNDO,
            menu::REDO,
            menu::CUT,
            menu::COPY,
            menu::PASTE,
            menu::SELECT_ALL,
            menu::SELECT_LINE,
            menu::SELECT_NEXT,
            menu::DUPLICATE_LINE,
            menu::DELETE_LINE,
            menu::TOGGLE_COMMENT,
        ] {
            let _ = shortcut(chord);
        }
    }
}
