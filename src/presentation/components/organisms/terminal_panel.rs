//! The built-in terminal: a header with the session tabs, and a grid rendered
//! from the active session's [`TerminalSnapshot`].
//!
//! **Rendering.** One `LayoutJob` per visible row, with runs of cells that share
//! a colour and style merged into a single section. Painting cell by cell would
//! re-shape every glyph every frame; this way the cost is proportional to the
//! number of *runs*, and the font atlas does the rest. Backgrounds are painted
//! underneath as rectangles, again merged into runs.
//!
//! **Input.** While the grid has focus every key is translated to the bytes a
//! terminal would send and handed to the PTY — including `Ctrl+C`, which is why
//! copy is bound to `⌘C` / `Ctrl+Shift+C` instead. The editor's own shortcuts
//! are kept out of the way by `editor_window`, which stops consuming them while
//! the terminal is focused.

use eframe::egui::{self, RichText, text::LayoutJob};

use crate::domain::terminal::{Rgb, TerminalCell, TerminalSize, grid_size};
use crate::presentation::terminal_controller::{
    self, SessionId, SessionKind, TerminalRequests, TerminalState,
};
use crate::presentation::theme;
use egui_phosphor::regular as ph;

/// Where the terminal's keyboard focus lives. A single id: only the active
/// session is ever on screen.
fn focus_id() -> egui::Id {
    egui::Id::new("terminal_grid")
}

/// True while the terminal owns the keyboard — `editor_window` asks so it can
/// leave the editor's shortcuts alone (D9).
pub fn has_focus(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.has_focus(focus_id()))
}

pub fn terminal_panel(ui: &mut egui::Ui, state: &mut TerminalState) -> TerminalRequests {
    let mut requests = terminal_controller::tick(state);

    header(ui, state);

    egui::Frame::new()
        .fill(theme::PLOT_FRAME_BACKGROUND)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height().max(0.0));
            body(ui, state, &mut requests);
        });

    requests
}

/// "TERMINAL", the session tabs, and the panel-level actions.
fn header(ui: &mut egui::Ui, state: &mut TerminalState) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("TERMINAL")
                .font(theme::small_font())
                .color(theme::MUTED_ON_DARK),
        );
        ui.add_space(8.0);

        let mut activate: Option<SessionId> = None;
        let mut close: Option<SessionId> = None;

        egui::ScrollArea::horizontal()
            .id_salt("terminal_tabs")
            .max_width(ui.available_width() - 60.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, session) in state.sessions.iter().enumerate() {
                        let active = index == state.active;
                        let label = tab_label(session);

                        if ui
                            .add(theme::selectable_button(&label, active))
                            .on_hover_text(session.command.program.clone())
                            .clicked()
                        {
                            activate = Some(session.id);
                        }
                        if ui
                            .add(theme::selectable_button(ph::X, false))
                            .on_hover_text("Close session")
                            .clicked()
                        {
                            close = Some(session.id);
                        }
                        ui.add_space(6.0);
                    }
                });
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(theme::selectable_button(ph::PLUS, false))
                .on_hover_text("New session")
                .clicked()
            {
                terminal_controller::open_shell(state);
            }
            if ui
                .add_enabled(
                    state.active_session().is_some(),
                    theme::selectable_button(ph::TRASH, false),
                )
                .on_hover_text("Clear")
                .clicked()
            {
                terminal_controller::clear_active(state);
            }
        });

        if let Some(id) = activate {
            terminal_controller::activate(state, id);
        }
        if let Some(id) = close {
            terminal_controller::close(state, id);
        }
    });
    ui.add_space(2.0);
}

/// A tab's caption: its title, with a build's outcome as a leading glyph.
///
/// The glyphs come from **Phosphor**, not from literal `●`/`✓`/`✕`. Those live
/// outside the bundled text fonts and outside Phosphor's private-use range, so
/// writing them renders a tofu box — the same trap `status_bar::unsaved_segment`
/// documents, which is why it paints its dot instead.
fn tab_label(session: &crate::presentation::terminal_controller::Session) -> String {
    use crate::domain::terminal::BuildStatus;
    let marker = match (session.kind, session.status) {
        (SessionKind::Build(_), Some(BuildStatus::Running)) => Some(ph::SPINNER_GAP),
        (SessionKind::Build(_), Some(BuildStatus::Succeeded)) => Some(ph::CHECK_CIRCLE),
        (SessionKind::Build(_), Some(BuildStatus::Failed(_))) => Some(ph::X_CIRCLE),
        _ => None,
    };
    match marker {
        Some(marker) => format!("{marker} {}", session.title),
        None => session.title.clone(),
    }
}

/// The grid, or whatever stands in for it: an empty panel, a spawn failure, or a
/// child that has exited.
fn body(ui: &mut egui::Ui, state: &mut TerminalState, requests: &mut TerminalRequests) {
    // The grid is sized from the panel, so the first session is opened here
    // rather than at construction — it starts at the real size and never has to
    // reflow on its first frame.
    let (cell_width, cell_height) = cell_metrics(ui);
    let available = ui.available_size();
    let (cols, rows) = grid_size(available.x, available.y, cell_width, cell_height);
    terminal_controller::resize(
        state,
        TerminalSize {
            cols,
            rows,
            cell_width: cell_width.round() as u16,
            cell_height: cell_height.round() as u16,
        },
    );
    terminal_controller::ensure_open(state);

    // Windows without git-bash: builds cannot run, but the PowerShell session
    // below is perfectly usable — so this is a strip above the grid rather than
    // something that replaces it.
    if state.missing_git_bash {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} `make` needs Git for Windows.",
                    ph::WARNING_CIRCLE
                ))
                .font(theme::small_font())
                .color(theme::UNSAVED_DOT),
            );
            ui.hyperlink_to(
                RichText::new("Install it").font(theme::small_font()),
                crate::domain::terminal::GIT_FOR_WINDOWS_URL,
            );
        });
        ui.add_space(4.0);
    }

    let Some(session) = state.active_session() else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("No terminal session — press + to open one")
                    .font(egui::FontId::monospace(theme::FONT_BODY))
                    .color(theme::MUTED_ON_DARK),
            );
        });
        return;
    };

    if let Some(error) = session.error.clone() {
        let id = session.id;
        notice(ui, &error, theme::ERROR_COLOR, "Retry", || {
            terminal_controller::restart(state, id)
        });
        return;
    }

    // A child that has exited leaves its output behind: the emulator is still
    // alive, holding the whole scrollback. So the notice is a strip *above* the
    // grid, never a replacement for it — a failed build's output is exactly what
    // the user needs at the moment it stops running.
    if let Some(text) = session.exit_notice() {
        let failed = matches!(
            (session.kind, session.status),
            (
                SessionKind::Build(_),
                Some(crate::domain::terminal::BuildStatus::Failed(_))
            )
        );
        let id = session.id;
        let restart = exit_strip(ui, &text, failed);
        if restart {
            terminal_controller::restart(state, id);
            return;
        }
    }

    let Some(snapshot) = terminal_controller::active_snapshot(state) else {
        return;
    };

    // Exactly **one** widget covers the grid, and it owns `focus_id()`.
    //
    // Two widgets over the same rect would be a silent trap: the one registered
    // later wins the hit test, so an `allocate_response` here plus a separate
    // `interact` for the focus id would leave the first response's `clicked()`
    // permanently false — the terminal could never be focused and would swallow
    // every keystroke. Allocating with `hover` and then interacting with our own
    // id keeps it to one.
    //
    // `Sense::click_and_drag()` already implies `FOCUSABLE`, and interacting
    // every frame is what keeps egui's dead-man's switch (`Focus::end_pass`)
    // from dropping the focus of a widget it thinks has disappeared.
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ui.available_height()),
        egui::Sense::hover(),
    );
    let response = ui.interact(rect, focus_id(), egui::Sense::click_and_drag());
    let origin = rect.min;

    paint_grid(ui, &snapshot, origin, cell_width, cell_height);

    // Clicking anywhere in the grid takes the keyboard, the same way clicking
    // into the code editor does.
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }

    handle_mouse(ui, state, &response, origin, cell_width, cell_height);

    // `has_focus` here rather than `Response::has_focus`, which also requires
    // `input.focused` — a viewport-level flag whose value for an immediate child
    // viewport (which the Editor window is) would silently gate out every
    // keystroke. egui's own `TextEdit` gates on the memory check alone.
    if has_focus(ui.ctx()) {
        // Claim the keys egui otherwise reserves for focus navigation. Without
        // this, Tab moves focus to the next widget instead of completing a
        // filename, the arrows walk the widget tree instead of shell history,
        // and Escape drops focus instead of leaving vim's insert mode — none of
        // which would ever reach the PTY. `TextEdit::lock_focus` does the same
        // thing for the same reason.
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                focus_id(),
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
        handle_keys(ui, state, requests);
    }

    handle_scroll(ui, state, &response);
}

/// One compact row reporting that the child has exited, with a Restart button.
/// Returns whether Restart was pressed.
///
/// Deliberately thin: it sits above the grid rather than replacing it, so the
/// scrollback stays readable, selectable and copyable after the process is gone.
fn exit_strip(ui: &mut egui::Ui, message: &str, failed: bool) -> bool {
    let mut restart = false;
    ui.horizontal(|ui| {
        let color = if failed {
            theme::ERROR_COLOR
        } else {
            theme::MUTED_ON_DARK
        };
        ui.label(
            RichText::new(message)
                .font(egui::FontId::monospace(theme::FONT_BODY))
                .color(color),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            restart = ui.add(theme::selectable_button("Restart", false)).clicked();
        });
    });
    ui.add_space(4.0);
    restart
}

/// A centred message with a single action — used for a session that could not
/// start at all, where there is no grid to show behind it.
fn notice(
    ui: &mut egui::Ui,
    message: &str,
    color: egui::Color32,
    action: &str,
    mut on_action: impl FnMut(),
) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new(message)
                .font(egui::FontId::monospace(theme::FONT_BODY))
                .color(color),
        );
        ui.add_space(6.0);
        if ui.add(theme::selectable_button(action, false)).clicked() {
            on_action();
        }
    });
}

/// The pixel size of one monospace cell.
///
/// Measured from the code font's own advance for `M` rather than assumed, so the
/// grid stays aligned if the font or its size ever changes.
fn cell_metrics(ui: &egui::Ui) -> (f32, f32) {
    let font = egui::FontId::monospace(theme::FONT_BODY);
    ui.fonts_mut(|fonts| {
        let width = fonts.glyph_width(&font, 'M');
        let height = fonts.row_height(&font);
        (width.max(1.0), height.max(1.0))
    })
}

/// Paints one frame of the grid: backgrounds first as merged rectangles, then a
/// `LayoutJob` per row with runs merged by colour and style.
fn paint_grid(
    ui: &egui::Ui,
    snapshot: &crate::domain::terminal::TerminalSnapshot,
    origin: egui::Pos2,
    cell_width: f32,
    cell_height: f32,
) {
    let painter = ui.painter();
    let font = egui::FontId::monospace(theme::FONT_BODY);
    let default_bg = theme::PLOT_FRAME_BACKGROUND;

    for row in 0..snapshot.rows {
        let cells = snapshot.row(row);
        if cells.is_empty() {
            continue;
        }
        let y = origin.y + row as f32 * cell_height;

        // -- backgrounds, as runs ------------------------------------------
        let mut run_start = 0usize;
        for column in 0..=cells.len() {
            let ends = column == cells.len()
                || background_of(&cells[column]) != background_of(&cells[run_start]);
            if !ends {
                continue;
            }
            let color = background_of(&cells[run_start]);
            if color != default_bg {
                let x = origin.x + run_start as f32 * cell_width;
                let width = (column - run_start) as f32 * cell_width;
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, cell_height)),
                    0.0,
                    color,
                );
            }
            run_start = column;
        }

        // -- text, one job per row -----------------------------------------
        let mut job = LayoutJob::default();
        let mut text = String::with_capacity(cells.len());
        let mut run_start = 0usize;
        for column in 0..=cells.len() {
            let ends = column == cells.len() || !same_run(&cells[column], &cells[run_start]);
            if !ends {
                continue;
            }

            let run: String = cells[run_start..column].iter().map(|c| c.c).collect();
            // Trailing blanks carry no ink; skipping them keeps the job small on
            // a mostly-empty screen, which is the common case.
            if !run.trim().is_empty() {
                let start = text.len();
                text.push_str(&run);
                job.sections.push(egui::text::LayoutSection {
                    leading_space: 0.0,
                    byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(text.len()),
                    format: format_of(&cells[run_start], &font),
                });
            } else {
                // Keep the columns aligned by advancing through the gap.
                text.push_str(&run);
                let start = text.len() - run.len();
                job.sections.push(egui::text::LayoutSection {
                    leading_space: 0.0,
                    byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(text.len()),
                    format: egui::TextFormat {
                        font_id: font.clone(),
                        color: egui::Color32::TRANSPARENT,
                        ..Default::default()
                    },
                });
            }
            run_start = column;
        }

        if !text.trim().is_empty() {
            job.text = text;
            job.wrap.max_width = f32::INFINITY;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            painter.galley(egui::pos2(origin.x, y), galley, theme::LABEL_ON_DARK);
        }
    }

    // -- cursor -------------------------------------------------------------
    if let Some((col, row)) = snapshot.cursor {
        let rect = egui::Rect::from_min_size(
            egui::pos2(
                origin.x + col as f32 * cell_width,
                origin.y + row as f32 * cell_height,
            ),
            egui::vec2(cell_width, cell_height),
        );
        // A hollow caret when unfocused, filled when focused — the same hint
        // every terminal gives about where typing will land.
        if has_focus(ui.ctx()) {
            painter.rect_filled(rect, 0.0, theme::LABEL_ON_DARK.gamma_multiply(0.55));
        } else {
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, theme::MUTED_ON_DARK),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// Selection wins over the cell's own background, so a highlighted region reads
/// as one block regardless of what the program coloured it.
fn background_of(cell: &TerminalCell) -> egui::Color32 {
    if cell.selected {
        theme::TERMINAL_SELECTION
    } else {
        theme::terminal_color(cell.bg)
    }
}

/// Whether two cells can share a layout section.
fn same_run(a: &TerminalCell, b: &TerminalCell) -> bool {
    a.fg == b.fg && a.style == b.style && a.selected == b.selected
}

fn format_of(cell: &TerminalCell, font: &egui::FontId) -> egui::TextFormat {
    egui::TextFormat {
        font_id: font.clone(),
        color: foreground_of(cell),
        underline: if cell.style.underline {
            egui::Stroke::new(1.0, foreground_of(cell))
        } else {
            egui::Stroke::NONE
        },
        strikethrough: if cell.style.strikeout {
            egui::Stroke::new(1.0, foreground_of(cell))
        } else {
            egui::Stroke::NONE
        },
        italics: cell.style.italic,
        ..Default::default()
    }
}

/// Bold is rendered as a brighter foreground rather than a heavier face: the app
/// ships one monospace weight, and a synthesised bold would break the cell grid.
fn foreground_of(cell: &TerminalCell) -> egui::Color32 {
    let color = theme::terminal_color(cell.fg);
    if cell.style.bold {
        brighten(cell.fg)
    } else {
        color
    }
}

fn brighten(rgb: Rgb) -> egui::Color32 {
    let lift = |c: u8| ((c as u16 * 5 / 4).min(255)) as u8;
    egui::Color32::from_rgb(lift(rgb.r), lift(rgb.g), lift(rgb.b))
}

/// Mouse selection: drag to select, and clicking clears whatever was selected.
fn handle_mouse(
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

    // A plain click (no drag) dismisses the selection, as terminals do.
    if response.clicked() {
        inner.select(None);
        let _ = ui;
    }
}

/// Wheel scrolling moves the grid's own viewport into the scrollback rather than
/// an egui `ScrollArea` — the grid owns `display_offset`, and two scroll models
/// over one buffer would fight.
fn handle_scroll(ui: &egui::Ui, state: &TerminalState, response: &egui::Response) {
    if !response.hovered() {
        return;
    }
    let delta = ui.input(|input| input.smooth_scroll_delta.y);
    if delta.abs() < 1.0 {
        return;
    }
    let (_, cell_height) = cell_metrics(ui);
    let lines = (delta / cell_height).round() as i32;
    if lines != 0
        && let Some(inner) = state.active_session().and_then(|s| s.inner.as_ref())
    {
        inner.scroll(lines);
    }
}

/// Translates this frame's key and text events into the bytes a terminal sends.
fn handle_keys(ui: &egui::Ui, state: &mut TerminalState, requests: &mut TerminalRequests) {
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
                // Copy and paste first: on Windows and Linux these are
                // Ctrl+Shift+…, because plain Ctrl+C has to stay SIGINT.
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

/// `⌘C` on macOS, `Ctrl+Shift+C` elsewhere.
fn is_copy(key: egui::Key, modifiers: &egui::Modifiers) -> bool {
    key == egui::Key::C
        && modifiers.command
        && (cfg!(target_os = "macos") || modifiers.shift)
        && !modifiers.alt
}

/// `⌘V` on macOS, `Ctrl+Shift+V` elsewhere.
fn is_paste(key: egui::Key, modifiers: &egui::Modifiers) -> bool {
    key == egui::Key::V
        && modifiers.command
        && (cfg!(target_os = "macos") || modifiers.shift)
        && !modifiers.alt
}

/// The bytes a terminal sends for a key press.
///
/// Returns `None` for keys egui also delivers as `Event::Text` (ordinary
/// characters), so they are not sent twice.
pub fn key_bytes(key: egui::Key, modifiers: &egui::Modifiers) -> Option<Vec<u8>> {
    use egui::Key as K;

    // Control characters: Ctrl+A..Ctrl+Z are 0x01..0x1a. `ctrl` rather than
    // `command`, so ⌘C on macOS never becomes SIGINT.
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

    // Alt-prefixed keys send ESC first, which is how a terminal encodes Meta.
    if modifiers.alt {
        let mut escaped = vec![0x1b];
        escaped.extend_from_slice(bytes);
        return Some(escaped);
    }
    Some(bytes.to_vec())
}

/// The control byte for `Ctrl+<letter>`, e.g. `Ctrl+C` → `0x03`.
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
    use crate::domain::terminal::CellStyle;
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
        // Sending these here too would type every letter twice.
        assert_eq!(key_bytes(Key::A, &Modifiers::NONE), None);
        assert_eq!(key_bytes(Key::Num1, &Modifiers::NONE), None);
    }

    #[test]
    fn copy_never_collides_with_the_interrupt() {
        // On every platform, a bare Ctrl+C must reach the child as SIGINT.
        assert!(!is_copy(Key::C, &Modifiers::CTRL));
        assert_eq!(key_bytes(Key::C, &Modifiers::CTRL), Some(vec![0x03]));

        // And the platform's copy chord must not be sent to the child.
        let copy = if cfg!(target_os = "macos") {
            Modifiers::COMMAND
        } else {
            Modifiers::COMMAND.plus(Modifiers::SHIFT)
        };
        assert!(is_copy(Key::C, &copy));
        assert!(is_paste(Key::V, &copy));
    }

    #[test]
    fn a_run_breaks_on_colour_style_or_selection() {
        let base = TerminalCell {
            c: 'a',
            fg: Rgb::new(1, 2, 3),
            bg: Rgb::new(0, 0, 0),
            style: CellStyle::default(),
            selected: false,
        };

        // Same everything but the character: one run.
        let mut other = base;
        other.c = 'b';
        assert!(same_run(&base, &other));

        let mut recoloured = base;
        recoloured.fg = Rgb::new(9, 9, 9);
        assert!(!same_run(&base, &recoloured));

        let mut bolded = base;
        bolded.style.bold = true;
        assert!(!same_run(&base, &bolded));

        let mut selected = base;
        selected.selected = true;
        assert!(!same_run(&base, &selected));
    }

    #[test]
    fn selection_overrides_the_cells_own_background() {
        let mut cell = TerminalCell {
            c: ' ',
            fg: Rgb::new(0, 0, 0),
            bg: Rgb::new(0x40, 0, 0),
            style: CellStyle::default(),
            selected: false,
        };
        assert_eq!(background_of(&cell), theme::terminal_color(cell.bg));
        cell.selected = true;
        assert_eq!(background_of(&cell), theme::TERMINAL_SELECTION);
    }
}
