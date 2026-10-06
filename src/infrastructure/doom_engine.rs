use neurodoom::{Button, ClassicEngine, PeerId, PlayerAction};

use crate::application::ports::{DoomError, DoomGame, DoomPort};
use crate::domain::doom::{DOOM_MAP, DoomControls, doom_command};

static DOOM_WAD: &[u8] = include_bytes!("../../assets/doom1.wad");

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

    #[test]
    fn e1m1_starts_from_the_embedded_wad_and_renders_a_frame() {
        let game = NeurodoomEngine::new().start();
        let mut game = game.expect("the embedded WAD must always start");
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
