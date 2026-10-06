use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::application::ports::{DoomGame, DoomPort};
use crate::domain::doom::{DoomControls, due_ticks};

pub struct DoomState {
    port: Rc<dyn DoomPort>,
    pub game: Option<Box<dyn DoomGame>>,
    pub error: Option<String>,
    pub focus_requested: bool,
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

        state.last_tick = Instant::now() - Duration::from_millis(60);
        tick(&mut state, DoomControls::default());
        assert!(
            (1..=3).contains(&port.ticks.get()),
            "≈60 ms owes about two 35 Hz ticks, got {}",
            port.ticks.get()
        );
    }
}
