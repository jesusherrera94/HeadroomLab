//! The DOOM.666 easter egg's policy: which file summons it, which map it runs,
//! and how held controls become the engine's per-tick command.
//!
//! Pure, like the rest of the domain — no engine, no egui, no clock. The
//! numbers in [`doom_command`] are vanilla Doom's own `ticcmd` values
//! (`forwardmove`/`sidemove`/`angleturn` in the original `g_game.c`), which is
//! why walking and turning feel right rather than approximately right.

use std::path::Path;
use std::time::Duration;

/// The file whose click opens the game instead of an editor tab. Exact match,
/// case-sensitive: the ritual has to be performed correctly.
pub const DOOM_FILE_NAME: &str = "DOOM.666";

/// The one map this easter egg runs. Reaching its exit ends the game rather
/// than leaking into E1M2.
pub const DOOM_MAP: &str = "E1M1";

/// The engine renders a fixed 320×200 frame; the window scales it up.
pub const DOOM_SCREEN_WIDTH: usize = 320;
pub const DOOM_SCREEN_HEIGHT: usize = 200;

/// Doom's simulation runs at exactly 35 Hz — one tick every 1/35 s. The UI
/// repaints faster than this; [`due_ticks`] converts wall time into whole
/// ticks so the game neither slow-motions nor fast-forwards with the frame
/// rate.
pub const DOOM_TICK: Duration = Duration::from_nanos(1_000_000_000 / 35);

/// A stall (window dragged, app hitched) accumulates owed ticks; running them
/// all would fast-forward the world while the player wasn't looking. Cap the
/// catch-up and drop the rest.
pub const MAX_CATCHUP_TICKS: u32 = 4;

/// Whether clicking this path should open the game window instead of a tab.
pub fn is_doom_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(DOOM_FILE_NAME)
}

/// How many simulation ticks are due after `elapsed` of wall time, carrying
/// the remainder in `accumulator` so sub-tick frames add up instead of being
/// rounded away — the same reasoning as the terminal's scroll carry.
pub fn due_ticks(accumulator: &mut Duration, elapsed: Duration) -> u32 {
    *accumulator += elapsed;
    let mut ticks = 0;
    while *accumulator >= DOOM_TICK {
        *accumulator -= DOOM_TICK;
        ticks += 1;
    }
    if ticks > MAX_CATCHUP_TICKS {
        // The dropped ticks are gone on purpose; owing them would replay the
        // stall at high speed.
        *accumulator = Duration::ZERO;
        return MAX_CATCHUP_TICKS;
    }
    ticks
}

/// The controls held during one tick. The view decides which keys mean what;
/// this type is what crosses the port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DoomControls {
    pub forward: bool,
    pub backward: bool,
    pub strafe_left: bool,
    pub strafe_right: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub run: bool,
    pub attack: bool,
    pub use_action: bool,
    /// Keyboard weapon slot `1..=6`, if one was pressed this tick.
    pub weapon_slot: Option<u8>,
}

/// One tick's command in the engine's own units.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DoomCommand {
    pub forward_move: i8,
    pub side_move: i8,
    pub angle_turn: i16,
    pub attack: bool,
    pub use_action: bool,
    /// Engine weapon index, `0` for "no change".
    pub weapon_select: u8,
}

// Vanilla `g_game.c`: forwardmove {25, 50}, sidemove {24, 40}, angleturn
// {640, 1280} (per tick, in BAM >> 16).
const FORWARD_MOVE: [i8; 2] = [25, 50];
const SIDE_MOVE: [i8; 2] = [24, 40];
const ANGLE_TURN: [i16; 2] = [640, 1280];

/// Resolves held controls into a command. Opposing directions cancel, exactly
/// as holding both arrows does in vanilla Doom.
pub fn doom_command(controls: DoomControls) -> DoomCommand {
    let speed = usize::from(controls.run);
    let mut command = DoomCommand {
        attack: controls.attack,
        use_action: controls.use_action,
        weapon_select: controls.weapon_slot.map_or(0, weapon_for_slot),
        ..DoomCommand::default()
    };

    if controls.forward {
        command.forward_move += FORWARD_MOVE[speed];
    }
    if controls.backward {
        command.forward_move -= FORWARD_MOVE[speed];
    }
    if controls.strafe_right {
        command.side_move += SIDE_MOVE[speed];
    }
    if controls.strafe_left {
        command.side_move -= SIDE_MOVE[speed];
    }
    // Positive turns left in Doom's angle space (angles grow anticlockwise).
    if controls.turn_left {
        command.angle_turn += ANGLE_TURN[speed];
    }
    if controls.turn_right {
        command.angle_turn -= ANGLE_TURN[speed];
    }

    command
}

/// Keyboard slot → engine weapon index. Slot 6 is the chainsaw (engine
/// index 8): the shareware WAD has no plasma rifle or BFG to select, so the
/// keyboard skips straight past their slots, as the crate's own example does.
fn weapon_for_slot(slot: u8) -> u8 {
    match slot {
        1..=5 => slot,
        6 => 8,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn only_the_exact_ritual_name_summons_the_game() {
        assert!(is_doom_file(Path::new("/proj/DOOM.666")));
        assert!(is_doom_file(&PathBuf::from("/proj/deep/nested/DOOM.666")));

        assert!(!is_doom_file(Path::new("/proj/doom.666")), "case matters");
        assert!(!is_doom_file(Path::new("/proj/DOOM.667")));
        assert!(!is_doom_file(Path::new("/proj/DOOM.666.txt")));
        assert!(!is_doom_file(Path::new("/proj/")));
    }

    #[test]
    fn opposing_directions_cancel() {
        let both = DoomControls {
            forward: true,
            backward: true,
            strafe_left: true,
            strafe_right: true,
            turn_left: true,
            turn_right: true,
            ..DoomControls::default()
        };
        let command = doom_command(both);
        assert_eq!(command.forward_move, 0);
        assert_eq!(command.side_move, 0);
        assert_eq!(command.angle_turn, 0);
    }

    #[test]
    fn movement_uses_vanilla_ticcmd_values_and_run_doubles_them() {
        let walk = doom_command(DoomControls {
            forward: true,
            strafe_right: true,
            turn_left: true,
            ..DoomControls::default()
        });
        assert_eq!(walk.forward_move, 25);
        assert_eq!(walk.side_move, 24);
        assert_eq!(walk.angle_turn, 640);

        let run = doom_command(DoomControls {
            forward: true,
            strafe_right: true,
            turn_left: true,
            run: true,
            ..DoomControls::default()
        });
        assert_eq!(run.forward_move, 50);
        assert_eq!(run.side_move, 40);
        assert_eq!(run.angle_turn, 1280);
    }

    #[test]
    fn turning_right_is_negative_backwards_is_negative() {
        let command = doom_command(DoomControls {
            backward: true,
            strafe_left: true,
            turn_right: true,
            ..DoomControls::default()
        });
        assert_eq!(command.forward_move, -25);
        assert_eq!(command.side_move, -24);
        assert_eq!(command.angle_turn, -640);
    }

    #[test]
    fn weapon_slots_map_to_engine_indices_with_six_as_the_chainsaw() {
        for slot in 1..=5u8 {
            let command = doom_command(DoomControls {
                weapon_slot: Some(slot),
                ..DoomControls::default()
            });
            assert_eq!(command.weapon_select, slot);
        }
        let saw = doom_command(DoomControls {
            weapon_slot: Some(6),
            ..DoomControls::default()
        });
        assert_eq!(saw.weapon_select, 8, "slot 6 is the chainsaw");

        let none = doom_command(DoomControls::default());
        assert_eq!(none.weapon_select, 0, "no press means no weapon change");
        let bogus = doom_command(DoomControls {
            weapon_slot: Some(9),
            ..DoomControls::default()
        });
        assert_eq!(bogus.weapon_select, 0);
    }

    #[test]
    fn buttons_pass_through() {
        let command = doom_command(DoomControls {
            attack: true,
            use_action: true,
            ..DoomControls::default()
        });
        assert!(command.attack);
        assert!(command.use_action);
    }

    #[test]
    fn ticks_accumulate_across_short_frames_instead_of_rounding_away() {
        let mut accumulator = Duration::ZERO;
        // A 60 fps frame is shorter than a 35 Hz tick: no tick yet…
        assert_eq!(due_ticks(&mut accumulator, Duration::from_millis(16)), 0);
        // …but two of them owe one.
        assert_eq!(due_ticks(&mut accumulator, Duration::from_millis(16)), 1);
    }

    #[test]
    fn a_long_stall_is_capped_not_fast_forwarded() {
        let mut accumulator = Duration::ZERO;
        let ticks = due_ticks(&mut accumulator, Duration::from_secs(3));
        assert_eq!(ticks, MAX_CATCHUP_TICKS);
        assert_eq!(
            accumulator,
            Duration::ZERO,
            "the dropped ticks must not stay owed"
        );
    }
}
