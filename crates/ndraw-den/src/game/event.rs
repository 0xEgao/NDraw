//! Viewer targeting for authoritative game events.

use ndraw_proto::{PlayerId, ServerMessage};

/// Recipients of one emitted server message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Every currently connected player.
    Everyone,
    /// Exactly one player, used for secrets and private feedback.
    Player(PlayerId),
    /// Every connected player except the named player.
    EveryoneExcept(PlayerId),
}

/// One authoritative message and its privacy boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedEvent {
    /// Intended recipients.
    pub audience: Audience,
    /// Wire-safe message to deliver.
    pub message: ServerMessage,
}

impl EmittedEvent {
    /// Creates a room-wide event.
    #[must_use]
    pub const fn everyone(message: ServerMessage) -> Self {
        Self {
            audience: Audience::Everyone,
            message,
        }
    }

    /// Creates a private event.
    #[must_use]
    pub const fn player(player_id: PlayerId, message: ServerMessage) -> Self {
        Self {
            audience: Audience::Player(player_id),
            message,
        }
    }

    /// Creates an event for existing peers after one player joins or resumes.
    #[must_use]
    pub const fn everyone_except(player_id: PlayerId, message: ServerMessage) -> Self {
        Self {
            audience: Audience::EveryoneExcept(player_id),
            message,
        }
    }
}
