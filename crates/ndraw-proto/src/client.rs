//! Messages accepted from WebSocket clients.

use crate::{
    ids::{PlayerId, TurnId},
    model::{DrawOp, DrawingVote, Hello},
};

/// A fully decoded client-to-server application message.
///
/// This enum intentionally does not implement `Serialize` or `Deserialize`.
/// The stable wire mapping is implemented explicitly in [`crate::codec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    /// First message on every newly upgraded WebSocket.
    Hello(Hello),
    /// Host request to leave the lobby and begin the game.
    StartGame,
    /// Drawer selects an offered word by zero-based index.
    PickWord {
        /// Index into the most recent private word options.
        choice: u8,
    },
    /// Drawer submits one canvas mutation.
    Draw(DrawOp),
    /// Guesser submits text for server-side evaluation.
    Guess {
        /// Plain-text guess.
        text: String,
    },
    /// Player submits a plain-text room message.
    Chat {
        /// Plain-text chat body.
        text: String,
    },
    /// Host requests removal of another player.
    KickPlayer {
        /// Player to remove.
        player_id: PlayerId,
    },
    /// Host requests a new game using the same room and roster.
    Rematch,
    /// Heartbeat response echoing the server nonce.
    Pong {
        /// Nonce received in [`crate::server::ServerMessage::Ping`].
        nonce: u32,
    },
    /// Adds, changes, or removes feedback on the just-completed drawing.
    VoteDrawing {
        /// Completed turn being rated.
        turn_id: TurnId,
        /// Desired vote, or `None` to remove the existing vote.
        vote: Option<DrawingVote>,
    },
}
