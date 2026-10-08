//! Turning mouse, wheel and keyboard input into terminal selection, scrollback
//! and the bytes a shell expects.

use eframe::egui;

use crate::presentation::terminal_controller::{self, TerminalRequests, TerminalState};

pub(super) fn handle_mouse(
    ui: &egui::Ui,
    state: &mut TerminalState,
    response: &egui::Response,
    origin: egui::Pos2,
    cell_width: f32,
    cell_height: f32,
) {
    let cell_at = |pos: egui::Pos2| -> (u16, u16) {
        let col = ((pos.x - origin.x) / cell_width).floor().max(0.0) as u16;
        let row = ((pos.y - origin.y) / cell_height).floor().max(0.0) as u16;
        (col, row)
    };

    let Some(session) = state.active_session_mut() else {
        return;
    };
    let Some(inner) = session.inner.as_ref() else {
        return;
    };

    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        session.drag_anchor = Some(cell_at(pos));
    }

    if response.dragged()
        && let (Some(anchor), Some(pos)) = (session.drag_anchor, response.interact_pointer_pos())
    {
        inner.select(Some((anchor, cell_at(pos))));
    }

    if response.drag_stopped() {
        session.drag_anchor = None;
    }

    if response.clicked() {
        inner.select(None);
        let _ = ui;
    }
}

pub(super) fn handle_scroll(
    ui: &egui::Ui,
    state: &mut TerminalState,
    response: &egui::Response,
    cell_height: f32,
) {
    if !response.hovered() {
        return;
    }

    state.scroll_carry += ui.input(|input| input.smooth_scroll_delta.y);

    let lines = (state.scroll_carry / cell_height).trunc();
    if lines == 0.0 {
        return;
    }
    state.scroll_carry -= lines * cell_height;

    if let Some(inner) = state.active_session().and_then(|s| s.inner.as_ref()) {
        inner.scroll(lines as i32);
    }
}

pub(super) fn handle_keys(
    ui: &egui::Ui,
    state: &mut TerminalState,
    requests: &mut TerminalRequests,
) {
    let events = ui.input(|input| input.events.clone());
    let mut out: Vec<u8> = Vec::new();

    for event in events {
        match event {
            egui::Event::Text(text) => out.extend_from_slice(text.as_bytes()),

            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if is_copy(key, &modifiers) {
                    if let Some(text) = state
                        .active_session()
                        .and_then(|s| s.inner.as_ref())
                        .and_then(|inner| inner.selection_text())
                        && !text.is_empty()
                    {
                        requests.copy = Some(text);
                    }
                    continue;
                }
                if is_paste(key, &modifiers) {
                    terminal_controller::paste_active(state);
                    continue;
                }

                if let Some(bytes) = key_bytes(key, &modifiers) {
                    out.extend_from_slice(&bytes);
                }
            }

            egui::Event::Paste(text) => out.extend_from_slice(text.as_bytes()),
            egui::Event::Copy => {
                if let Some(text) = state
                    .active_session()
                    .and_then(|s| s.inner.as_ref())
                    .and_then(|inner| inner.selection_text())
                {
                    requests.copy = Some(text);
                }
            }
            _ => {}
        }
    }

    if !out.is_empty() {
        terminal_controller::write_active(state, &out);
    }
}

fn is_copy(key: egui::Key, modifiers: &egui::Modifiers) -> bool {
    key == egui::Key::C
        && modifiers.command
        && (cfg!(target_os = "macos") || modifiers.shift)
        && !modifiers.alt
}

fn is_paste(key: egui::Key, modifiers: &egui::Modifiers) -> bool {
    key == egui::Key::V
        && modifiers.command
        && (cfg!(target_os = "macos") || modifiers.shift)
        && !modifiers.alt
}

pub fn key_bytes(key: egui::Key, modifiers: &egui::Modifiers) -> Option<Vec<u8>> {
    use egui::Key as K;

    if modifiers.ctrl && !modifiers.alt {
        if let Some(letter) = ctrl_letter(key) {
            return Some(vec![letter]);
        }
        match key {
            K::OpenBracket => return Some(vec![0x1b]), // Ctrl+[ is Escape
            K::Backslash => return Some(vec![0x1c]),
            K::CloseBracket => return Some(vec![0x1d]),
            _ => {}
        }
    }

    let bytes: &[u8] = match key {
        K::Enter => b"\r",
        K::Tab => b"\t",
        K::Backspace => b"\x7f",
        K::Escape => b"\x1b",
        K::Delete => b"\x1b[3~",
        K::Insert => b"\x1b[2~",
        K::Home => b"\x1b[H",
        K::End => b"\x1b[F",
        K::PageUp => b"\x1b[5~",
        K::PageDown => b"\x1b[6~",
        K::ArrowUp => b"\x1b[A",
        K::ArrowDown => b"\x1b[B",
        K::ArrowRight => b"\x1b[C",
        K::ArrowLeft => b"\x1b[D",
        K::F1 => b"\x1bOP",
        K::F2 => b"\x1bOQ",
        K::F3 => b"\x1bOR",
        K::F4 => b"\x1bOS",
        K::F5 => b"\x1b[15~",
        K::F6 => b"\x1b[17~",
        K::F7 => b"\x1b[18~",
        K::F8 => b"\x1b[19~",
        K::F9 => b"\x1b[20~",
        K::F10 => b"\x1b[21~",
        K::F11 => b"\x1b[23~",
        K::F12 => b"\x1b[24~",
        _ => return None,
    };

    if modifiers.alt {
        let mut escaped = vec![0x1b];
        escaped.extend_from_slice(bytes);
        return Some(escaped);
    }
    Some(bytes.to_vec())
}

fn ctrl_letter(key: egui::Key) -> Option<u8> {
    use egui::Key as K;
    let index = match key {
        K::A => 1,
        K::B => 2,
        K::C => 3,
        K::D => 4,
        K::E => 5,
        K::F => 6,
        K::G => 7,
        K::H => 8,
        K::I => 9,
        K::J => 10,
        K::K => 11,
        K::L => 12,
        K::M => 13,
        K::N => 14,
        K::O => 15,
        K::P => 16,
        K::Q => 17,
        K::R => 18,
        K::S => 19,
        K::T => 20,
        K::U => 21,
        K::V => 22,
        K::W => 23,
        K::X => 24,
        K::Y => 25,
        K::Z => 26,
        _ => return None,
    };
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Key, Modifiers};

    #[test]
    fn control_letters_map_to_their_control_bytes() {
        let ctrl = Modifiers::CTRL;
        assert_eq!(
            key_bytes(Key::C, &ctrl),
            Some(vec![0x03]),
            "Ctrl+C is SIGINT"
        );
        assert_eq!(key_bytes(Key::D, &ctrl), Some(vec![0x04]), "Ctrl+D is EOF");
        assert_eq!(key_bytes(Key::A, &ctrl), Some(vec![0x01]));
        assert_eq!(key_bytes(Key::Z, &ctrl), Some(vec![0x1a]));
    }

    #[test]
    fn the_editing_keys_send_their_escape_sequences() {
        let none = Modifiers::NONE;
        assert_eq!(key_bytes(Key::Enter, &none), Some(b"\r".to_vec()));
        assert_eq!(key_bytes(Key::Backspace, &none), Some(b"\x7f".to_vec()));
        assert_eq!(key_bytes(Key::ArrowUp, &none), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(Key::Delete, &none), Some(b"\x1b[3~".to_vec()));
        assert_eq!(key_bytes(Key::F5, &none), Some(b"\x1b[15~".to_vec()));
    }

    #[test]
    fn alt_prefixes_a_key_with_escape_the_way_meta_is_encoded() {
        assert_eq!(
            key_bytes(Key::ArrowLeft, &Modifiers::ALT),
            Some(b"\x1b\x1b[D".to_vec())
        );
    }

    #[test]
    fn ordinary_characters_are_left_to_the_text_event() {
        assert_eq!(key_bytes(Key::A, &Modifiers::NONE), None);
        assert_eq!(key_bytes(Key::Num1, &Modifiers::NONE), None);
    }

    #[test]
    fn copy_never_collides_with_the_interrupt() {
        assert!(!is_copy(Key::C, &Modifiers::CTRL));
        assert_eq!(key_bytes(Key::C, &Modifiers::CTRL), Some(vec![0x03]));

        let copy = if cfg!(target_os = "macos") {
            Modifiers::COMMAND
        } else {
            Modifiers::COMMAND.plus(Modifiers::SHIFT)
        };
        assert!(is_copy(Key::C, &copy));
        assert!(is_paste(Key::V, &copy));
    }
}
