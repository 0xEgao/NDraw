//! Authenticated actions accepted by the pure game engine.

use ndraw_proto::{DrawOp, DrawingVote, PlayerId, TurnId};

/// A gameplay action paired with a player identity by the room actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerAction {
    /// Host starts the initial game.
    StartGame,
    /// Current drawer chooses one offered word by index.
    PickWord {
        /// Zero-based index into the private offer.
        choice: u8,
    },
    /// Current drawer mutates the authoritative canvas.
    Draw(DrawOp),
    /// Non-drawer submits a guess.
    Guess {
        /// Plain-text guess.
        text: String,
    },
    /// Player submits room chat.
    Chat {
        /// Plain-text chat body.
        text: String,
    },
    /// Host removes a player from the room.
    KickPlayer {
        /// Player to remove.
        player_id: PlayerId,
    },
    /// Host starts another game after game over.
    Rematch,
    /// Player rates the just-completed drawing or removes their rating.
    VoteDrawing {
        /// Completed turn being rated.
        turn_id: TurnId,
        /// Desired vote, or `None` to remove the current vote.
        vote: Option<DrawingVote>,
    },
}
