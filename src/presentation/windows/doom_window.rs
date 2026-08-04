//! The DOOM.666 window: the engine's 320×200 frame scaled up to fill the
//! viewport, and the overlays for the three ways a run ends (never started,
//! died, cleared).
//!
//! **Painting.** One texture, updated in place every frame — recreating it
//! would allocate a new GPU texture 35 times a second. Nearest-neighbour
//! filtering keeps the pixels crunchy instead of smearing them, and the image
//! is fitted to a 4:3 rect: Doom's 320×200 frame was drawn for 4:3 monitors
//! with non-square pixels, so stretching it there is what "correct" looks like.
//!
//! **Input.** Held keys are read from `keys_down` each frame rather than from
//! key events: the engine wants "what is held during this tick", not edges.
//! WASD/arrows move and turn, Space uses, E or Ctrl fires, Shift runs,
//! 1–6 pick a weapon — the classic bindings.

use eframe::egui::{self, RichText};

use crate::domain::doom::{DOOM_SCREEN_HEIGHT, DOOM_SCREEN_WIDTH, DOOM_TICK, DoomControls};
use crate::presentation::doom_controller::{self, DoomState};
use crate::presentation::theme;

pub fn show(ui: &mut egui::Ui, state: &mut DoomState) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::BLACK))
        .show(ui, |ui| {
            if let Some(error) = state.error.clone() {
                notice(ui, "the ritual failed", &error, "Retry", state);
                return;
            }

            doom_controller::tick(state, held_controls(ui));

            let Some(game) = state.game.as_ref() else {
                return;
            };

            paint_frame(ui, game.frame());

            if game.level_complete() {
                overlay(ui, "E1M1 CLEARED", "the only way out is through", state);
            } else if game.player_dead() {
                overlay(ui, "YOU DIED", "as everyone does, eventually", state);
            } else {
                // The simulation owes a tick every 1/35 s whether or not the
                // user touches anything; the app's 100 ms heartbeat is far too
                // slow for a game.
                ui.ctx().request_repaint_after(DOOM_TICK);
            }
        });
}

/// The controls held this frame. `keys_down`, not events: a held W must keep
/// walking across every tick, not just the one its press landed on.
fn held_controls(ui: &egui::Ui) -> DoomControls {
    use egui::Key as K;
    ui.input(|input| DoomControls {
        forward: input.key_down(K::W) || input.key_down(K::ArrowUp),
        backward: input.key_down(K::S) || input.key_down(K::ArrowDown),
        strafe_left: input.key_down(K::A),
        strafe_right: input.key_down(K::D),
        turn_left: input.key_down(K::ArrowLeft),
        turn_right: input.key_down(K::ArrowRight),
        run: input.modifiers.shift,
        attack: input.key_down(K::E) || input.modifiers.ctrl,
        use_action: input.key_down(K::Space),
        weapon_slot: [K::Num1, K::Num2, K::Num3, K::Num4, K::Num5, K::Num6]
            .into_iter()
            .position(|key| input.key_down(key))
            .map(|index| index as u8 + 1),
    })
}

/// Uploads this frame's pixels and paints them into the largest 4:3 rect that
/// fits the panel, letterboxed on black.
fn paint_frame(ui: &mut egui::Ui, frame: &[u8]) {
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [DOOM_SCREEN_WIDTH, DOOM_SCREEN_HEIGHT],
        frame,
    );
    let options = egui::TextureOptions::NEAREST;

    // The handle lives in egui's own storage rather than in `DoomState`, which
    // keeps the controller egui-free (the same boundary the terminal draws).
    // Looked up and created in separate context locks: `load_texture` reads
    // the context, so calling it from inside `data_mut`'s write lock would
    // deadlock.
    let id = egui::Id::new("doom_frame");
    let existing: Option<egui::TextureHandle> = ui.ctx().data(|data| data.get_temp(id));
    let texture = match existing {
        Some(mut texture) => {
            texture.set(image, options);
            texture
        }
        None => {
            let texture = ui.ctx().load_texture("doom_frame", image, options);
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        }
    };

    let available = ui.available_rect_before_wrap();
    let scale = (available.width() / 4.0).min(available.height() / 3.0);
    let rect = egui::Rect::from_center_size(
        available.center(),
        egui::vec2(scale * 4.0, scale * 3.0),
    );

    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// The end-of-run scrim: a title in the middle of the frame and one button
/// under it. Doom's own intermission palette — a red that means it.
fn overlay(ui: &mut egui::Ui, title: &str, subtitle: &str, state: &mut DoomState) {
    let rect = ui.max_rect();
    ui.painter()
        .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(140));

    let mut restart = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(rect.height() * 0.35);
            ui.label(
                RichText::new(title)
                    .font(egui::FontId::monospace(28.0))
                    .strong()
                    .color(theme::ERROR_COLOR),
            );
            ui.label(
                RichText::new(subtitle)
                    .font(egui::FontId::monospace(theme::FONT_BODY))
                    .color(theme::MUTED_ON_DARK),
            );
            ui.add_space(10.0);
            restart = ui
                .add(theme::selectable_button("Rip and tear again", false))
                .clicked();
        });
    });
    if restart {
        doom_controller::restart(state);
    }
}

/// A centred message for a game that never started — no frame to draw behind.
fn notice(ui: &mut egui::Ui, title: &str, detail: &str, action: &str, state: &mut DoomState) {
    let mut retry = false;
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.35);
        ui.label(
            RichText::new(title)
                .font(egui::FontId::monospace(theme::FONT_TITLE))
                .color(theme::ERROR_COLOR),
        );
        ui.label(
            RichText::new(detail)
                .font(egui::FontId::monospace(theme::FONT_BODY))
                .color(theme::MUTED_ON_DARK),
        );
        ui.add_space(8.0);
        retry = ui.add(theme::selectable_button(action, false)).clicked();
    });
    if retry {
        doom_controller::restart(state);
    }
}
