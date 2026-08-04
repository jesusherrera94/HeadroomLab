//! State and behaviour for the DOOM.666 window: the running game, its 35 Hz
//! cadence, and the restart that both death and victory offer.
//!
//! Free of egui, like the other controllers — the window draws, this decides.
//! The tick cadence lives in `domain::doom::due_ticks`; this module only feeds
//! it the clock.

use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::application::ports::{DoomGame, DoomPort};
use crate::domain::doom::{DoomControls, due_ticks};

pub struct DoomState {
    /// Kept so death and victory can offer "again".
    port: Rc<dyn DoomPort>,
    /// `None` when the engine failed to start; `error` says why.
    pub game: Option<Box<dyn DoomGame>>,
    pub error: Option<String>,
    /// Bring the window to the front next frame.
    pub focus_requested: bool,
    /// When the simulation last advanced, plus the sub-tick remainder — a
    /// 60 fps frame is shorter than a 35 Hz tick, so fractions must carry.
    last_tick: Instant,
    accumulator: Duration,
}

impl DoomState {
    pub fn open(port: Rc<dyn DoomPort>) -> Self {
        let (game, error) = start(port.as_ref());
        Self {
            port,
            game,
            error,
            focus_requested: false,
            last_tick: Instant::now(),
            accumulator: Duration::ZERO,
        }
    }
}

fn start(port: &dyn DoomPort) -> (Option<Box<dyn DoomGame>>, Option<String>) {
    match port.start() {
        Ok(game) => (Some(game), None),
        Err(e) => (None, Some(e.0)),
    }
}

/// Advances the simulation by however many whole ticks the wall clock owes,
/// holding `controls` for all of them.
///
/// The world keeps ticking after the player dies — the corpse-eye view is part
/// of the game — but freezes on victory: the exit has been crossed, and the
/// frame on screen is the trophy.
pub fn tick(state: &mut DoomState, controls: DoomControls) {
    let now = Instant::now();
    let elapsed = now - state.last_tick;
    state.last_tick = now;

    let Some(game) = state.game.as_mut() else {
        return;
    };
    if game.level_complete() {
        return;
    }

    for _ in 0..due_ticks(&mut state.accumulator, elapsed) {
        game.tick(controls);
    }
}

/// A fresh E1M1, whether the last one ended in an exit, a corpse, or an engine
/// that never started.
pub fn restart(state: &mut DoomState) {
    let (game, error) = start(state.port.as_ref());
    state.game = game;
    state.error = error;
    state.last_tick = Instant::now();
    state.accumulator = Duration::ZERO;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::DoomError;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A game that counts its ticks and reports whatever the test says.
    struct FakeGame {
        ticks: Rc<Cell<u32>>,
        complete: Rc<Cell<bool>>,
        frame: Vec<u8>,
    }

    impl DoomGame for FakeGame {
        fn tick(&mut self, _: DoomControls) {
            self.ticks.set(self.ticks.get() + 1);
        }
        fn frame(&self) -> &[u8] {
            &self.frame
        }
        fn level_complete(&self) -> bool {
            self.complete.get()
        }
        fn player_dead(&self) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct FakePort {
        starts: Cell<u32>,
        fail: Cell<bool>,
        ticks: Rc<Cell<u32>>,
        complete: Rc<Cell<bool>>,
    }

    impl DoomPort for FakePort {
        fn start(&self) -> Result<Box<dyn DoomGame>, DoomError> {
            self.starts.set(self.starts.get() + 1);
            if self.fail.get() {
                return Err(DoomError("no WAD in the walls".into()));
            }
            Ok(Box::new(FakeGame {
                ticks: self.ticks.clone(),
                complete: self.complete.clone(),
                frame: Vec::new(),
            }))
        }
    }

    #[test]
    fn opening_starts_a_game_and_a_failure_becomes_the_error() {
        let port = Rc::new(FakePort::default());
        let state = DoomState::open(port.clone());
        assert!(state.game.is_some());
        assert!(state.error.is_none());

        port.fail.set(true);
        let state = DoomState::open(port);
        assert!(state.game.is_none());
        assert_eq!(state.error.as_deref(), Some("no WAD in the walls"));
    }

    #[test]
    fn restart_replaces_the_game_and_clears_an_old_error() {
        let port = Rc::new(FakePort::default());
        port.fail.set(true);
        let mut state = DoomState::open(port.clone());
        assert!(state.error.is_some());

        port.fail.set(false);
        restart(&mut state);
        assert!(state.game.is_some());
        assert!(state.error.is_none());
        assert_eq!(port.starts.get(), 2);
    }

    #[test]
    fn a_completed_level_freezes_instead_of_ticking_on() {
        let port = Rc::new(FakePort::default());
        let mut state = DoomState::open(port.clone());

        // Make a tick due, then complete the level: nothing may advance.
        port.complete.set(true);
        state.accumulator = Duration::from_secs(1);
        state.last_tick = Instant::now();
        tick(&mut state, DoomControls::default());
        assert_eq!(port.ticks.get(), 0, "victory is a freeze-frame");
    }

    #[test]
    fn owed_time_becomes_engine_ticks() {
        let port = Rc::new(FakePort::default());
        let mut state = DoomState::open(port.clone());

        // Pretend the last tick was ~2 doom-ticks ago.
        state.last_tick = Instant::now() - Duration::from_millis(60);
        tick(&mut state, DoomControls::default());
        assert!(
            (1..=3).contains(&port.ticks.get()),
            "≈60 ms owes about two 35 Hz ticks, got {}",
            port.ticks.get()
        );
    }
}
