//! Deterministic score calculations.

use crate::time::{GameDeadline, GameTime};

/// Maximum score awarded to an immediate correct guess.
pub const MAX_GUESS_SCORE: u32 = 500;

/// Minimum score awarded to a correct guess before the deadline.
pub const MIN_GUESS_SCORE: u32 = 50;

/// Calculates a time-weighted guess score with integer round-to-nearest.
#[must_use]
pub fn guess_score(deadline: GameDeadline, now: GameTime, duration_ms: u64) -> u32 {
    if duration_ms == 0 {
        return MIN_GUESS_SCORE;
    }
    let remaining = deadline.remaining_ms(now).min(duration_ms);
    let numerator = u64::from(MAX_GUESS_SCORE)
        .saturating_mul(remaining)
        .saturating_add(duration_ms / 2);
    let rounded = numerator / duration_ms;
    let bounded = rounded.clamp(u64::from(MIN_GUESS_SCORE), u64::from(MAX_GUESS_SCORE));
    bounded as u32
}
