//! Deterministic time values used by the pure game engine.

use std::time::Duration;

/// Milliseconds elapsed since a room actor started.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameTime(pub u64);

impl GameTime {
    /// Converts a duration into a saturating millisecond value.
    #[must_use]
    pub fn from_duration(duration: Duration) -> Self {
        match u64::try_from(duration.as_millis()) {
            Ok(milliseconds) => Self(milliseconds),
            Err(_) => Self(u64::MAX),
        }
    }

    /// Advances time without overflowing.
    #[must_use]
    pub const fn saturating_add(self, duration: Duration) -> Self {
        let milliseconds = duration.as_millis();
        let delta = if milliseconds > u64::MAX as u128 {
            u64::MAX
        } else {
            milliseconds as u64
        };
        Self(self.0.saturating_add(delta))
    }
}

/// Millisecond deadline in the room's monotonic time domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameDeadline(pub u64);

impl GameDeadline {
    /// Constructs a deadline relative to `now` using saturating arithmetic.
    #[must_use]
    pub fn after(now: GameTime, duration: Duration) -> Self {
        Self(now.saturating_add(duration).0)
    }

    /// Returns whether the deadline has elapsed.
    #[must_use]
    pub const fn is_due(self, now: GameTime) -> bool {
        now.0 >= self.0
    }

    /// Returns the time remaining before this deadline.
    #[must_use]
    pub const fn remaining_ms(self, now: GameTime) -> u64 {
        self.0.saturating_sub(now.0)
    }
}
