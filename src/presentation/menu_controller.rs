use eframe::egui;

use crate::domain::menu::{Chord, MenuCommand, TransportCommand, WindowId};
use crate::presentation::editor_controller::{self, EditorState, EntryKind};

#[derive(Default)]
pub struct MenuRequests {
    pub open_initial: Option<OpenInitial>,
    pub open_recent: Option<usize>,
    pub clear_recents: bool,
    pub quit: bool,
    pub close_window: bool,
    pub focus_window: Option<WindowId>,
    pub show_about: bool,
    pub open_help: bool,
    pub check_for_updates: bool,
    pub open_emulator: bool,
    pub build_run: bool,
    pub compile: bool,
    pub transport: Option<TransportCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenInitial {
    Picker,
    CreateModal,
}

pub fn dispatch(command: MenuCommand, editor: Option<&mut EditorState>) -> MenuRequests {
    let mut requests = MenuRequests::default();

    match command {
        MenuCommand::NewProject => requests.open_initial = Some(OpenInitial::CreateModal),
        MenuCommand::OpenProject => requests.open_initial = Some(OpenInitial::Picker),
        MenuCommand::OpenRecent(index) => requests.open_recent = Some(index),
        MenuCommand::ClearRecents => requests.clear_recents = true,
        MenuCommand::Quit => requests.quit = true,
        MenuCommand::CloseWindow => requests.close_window = true,
        MenuCommand::FocusWindow(window) => requests.focus_window = Some(window),
        MenuCommand::About => requests.show_about = true,
        MenuCommand::Help => requests.open_help = true,
        MenuCommand::CheckForUpdates => requests.check_for_updates = true,
        MenuCommand::OpenEmulator => requests.open_emulator = true,
        MenuCommand::BuildRun => requests.build_run = true,
        MenuCommand::Compile => requests.compile = true,
        MenuCommand::Transport(action) => requests.transport = Some(action),

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
        MenuCommand::Edit(command) => {
            if let Some(state) = editor {
                editor_controller::queue_command(state, command);
            }
        }

        MenuCommand::Predefined(_) => {}
    }

    requests
}

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
