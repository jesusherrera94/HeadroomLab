//! Keyboard shortcuts: the `Chord` type, how it renders per platform, and the
//! app-wide shortcut table.

use std::fmt;

use crate::domain::editing::EditorCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: &'static str,
}

impl Chord {
    pub const fn cmd(key: &'static str) -> Self {
        Self {
            command: true,
            shift: false,
            alt: false,
            key,
        }
    }

    pub const fn cmd_shift(key: &'static str) -> Self {
        Self {
            command: true,
            shift: true,
            alt: false,
            key,
        }
    }

    pub const fn cmd_alt(key: &'static str) -> Self {
        Self {
            command: true,
            shift: false,
            alt: true,
            key,
        }
    }

    pub const fn cmd_alt_shift(key: &'static str) -> Self {
        Self {
            command: true,
            shift: true,
            alt: true,
            key,
        }
    }

    pub const fn shift_alt(key: &'static str) -> Self {
        Self {
            command: false,
            shift: true,
            alt: true,
            key,
        }
    }

    pub const fn plain(key: &'static str) -> Self {
        Self {
            command: false,
            shift: false,
            alt: false,
            key,
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if cfg!(target_os = "macos") {
            if self.shift {
                f.write_str("⇧")?;
            }
            if self.alt {
                f.write_str("⌥")?;
            }
            if self.command {
                f.write_str("⌘")?;
            }
            f.write_str(mac_key(self.key))
        } else {
            if self.command {
                f.write_str("Ctrl+")?;
            }
            if self.shift {
                f.write_str("Shift+")?;
            }
            if self.alt {
                f.write_str("Alt+")?;
            }
            f.write_str(self.key)
        }
    }
}

fn mac_key(key: &str) -> &str {
    match key {
        "Down" => "↓",
        "Up" => "↑",
        other => other,
    }
}

pub const SAVE: Chord = Chord::cmd("S");
pub const SAVE_ALL: Chord = Chord::cmd_shift("S");
pub const FIND: Chord = Chord::cmd("F");
pub const NEW_PROJECT: Chord = Chord::cmd_shift("N");
pub const OPEN_PROJECT: Chord = Chord::cmd("O");
pub const NEW_FILE: Chord = Chord::cmd("N");
pub const NEW_FOLDER: Chord = Chord::cmd_alt_shift("N");
pub const CLOSE_TAB: Chord = Chord::cmd("W");
pub const CLOSE_WINDOW: Chord = Chord::cmd_shift("W");
pub const QUIT: Chord = Chord::cmd("Q");
pub const HIDE: Chord = Chord::cmd("H");
pub const HIDE_OTHERS: Chord = Chord::cmd_alt("H");
pub const BUILD_RUN: Chord = Chord::cmd("R");
pub const COMPILE: Chord = Chord::cmd("B");
pub const PLAY_PAUSE: Chord = Chord::plain("Space");

pub const UNDO: Chord = Chord::cmd("Z");
pub const REDO: Chord = Chord::cmd_shift("Z");
pub const CUT: Chord = Chord::cmd("X");
pub const COPY: Chord = Chord::cmd("C");
pub const PASTE: Chord = Chord::cmd("V");
pub const SELECT_ALL: Chord = Chord::cmd("A");
pub const SELECT_LINE: Chord = Chord::cmd("L");
pub const SELECT_NEXT: Chord = Chord::cmd("D");
pub const DUPLICATE_LINE: Chord = Chord::shift_alt("Down");
pub const DELETE_LINE: Chord = Chord::cmd_shift("K");
pub const TOGGLE_COMMENT: Chord = Chord::cmd("/");

pub fn chord_for(command: EditorCommand) -> Chord {
    match command {
        EditorCommand::Undo => UNDO,
        EditorCommand::Redo => REDO,
        EditorCommand::Cut => CUT,
        EditorCommand::Copy => COPY,
        EditorCommand::Paste => PASTE,
        EditorCommand::SelectAll => SELECT_ALL,
        EditorCommand::SelectLine => SELECT_LINE,
        EditorCommand::SelectNextOccurrence => SELECT_NEXT,
        EditorCommand::ToggleComment => TOGGLE_COMMENT,
        EditorCommand::DuplicateLine => DUPLICATE_LINE,
        EditorCommand::DeleteLine => DELETE_LINE,
    }
}
