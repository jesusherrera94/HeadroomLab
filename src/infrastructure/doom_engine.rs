//! `DoomPort` over `neurodoom`: a pure-Rust Doom engine driven one 35 Hz tick
//! at a time, rendering into a 320×200 RGBA buffer the window paints as a
//! texture.
//!
//! **Game data.** The shareware `doom1.wad` (v1.9, freely redistributable) is
//! embedded in the binary, so the easter egg works on a clean checkout with
//! nothing to download or configure. ~4 MB — the price of a ritual that always
//! answers.
//!
//! **No audio.** The engine is headless by design; the game runs silent. The
//! sound lumps are in the WAD, but there is nothing to play them.

use neurodoom::{Button, ClassicEngine, PeerId, PlayerAction};

use crate::application::ports::{DoomError, DoomGame, DoomPort};
use crate::domain::doom::{DOOM_MAP, DoomControls, doom_command};

/// Doom v1.9 shareware IWAD (MD5 `f0cefca49926d00903cf57551d901abe`).
static DOOM_WAD: &[u8] = include_bytes!("../../assets/doom1.wad");

/// Starts games of E1M1 from the embedded WAD.
pub struct NeurodoomEngine;

impl NeurodoomEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NeurodoomEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DoomPort for NeurodoomEngine {
    fn start(&self) -> Result<Box<dyn DoomGame>, DoomError> {
        let engine = ClassicEngine::new(DOOM_WAD, DOOM_MAP)
            .map_err(|e| DoomError(format!("could not start {DOOM_MAP}: {e:?}")))?;
        Ok(Box::new(NeurodoomGame { engine }))
    }
}

struct NeurodoomGame {
    engine: ClassicEngine,
}

impl DoomGame for NeurodoomGame {
    fn tick(&mut self, controls: DoomControls) {
        let command = doom_command(controls);
        let mut action = PlayerAction {
            forward_move: command.forward_move,
            side_move: command.side_move,
            angle_turn: command.angle_turn,
            weapon_select: command.weapon_select,
            ..PlayerAction::default()
        };
        if command.attack {
            action.buttons |= Button::Attack;
        }
        if command.use_action {
            action.buttons |= Button::Use;
        }
        self.engine.tick_single(PeerId(0), action);
    }

    fn frame(&self) -> &[u8] {
        self.engine.framebuffer()
    }

    fn level_complete(&self) -> bool {
        self.engine.level_complete()
    }

    fn player_dead(&self) -> bool {
        let world = self.engine.world();
        world
            .controlled_by(PeerId(0))
            .and_then(|id| world.get(id))
            .is_some_and(|player| player.health <= 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::doom::{DOOM_SCREEN_HEIGHT, DOOM_SCREEN_WIDTH};

    /// The one test worth running over the real engine and the real WAD: the
    /// embedded bytes actually parse, the map actually spawns, and a frame
    /// actually renders. A fake here would only prove the fake works.
    #[test]
    fn e1m1_starts_from_the_embedded_wad_and_renders_a_frame() {
        let game = NeurodoomEngine::new().start();
        let mut game = game.expect("the embedded WAD must always start");

        // A second of walking into Hangar's opening room.
        for _ in 0..35 {
            game.tick(DoomControls {
                forward: true,
                ..DoomControls::default()
            });
        }

        let frame = game.frame();
        assert_eq!(
            frame.len(),
            DOOM_SCREEN_WIDTH * DOOM_SCREEN_HEIGHT * 4,
            "one RGBA byte quartet per pixel"
        );
        assert!(
            frame.chunks_exact(4).any(|px| px[..3] != [0, 0, 0]),
            "a rendered E1M1 frame cannot be all black"
        );

        assert!(!game.level_complete(), "the exit is nowhere near the start");
        assert!(!game.player_dead(), "nothing at the start deals damage");
    }

    /// Firing must be safe to drive from the UI thread every tick — this is the
    /// path a held mouse-of-war E key exercises.
    #[test]
    fn shooting_and_turning_do_not_disturb_the_simulation() {
        let mut game = NeurodoomEngine::new()
            .start()
            .expect("the embedded WAD must always start");
        for _ in 0..10 {
            game.tick(DoomControls {
                attack: true,
                turn_left: true,
                run: true,
                ..DoomControls::default()
            });
        }
        assert!(!game.player_dead());
    }
}
