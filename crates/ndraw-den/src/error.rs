//! Domain, actor, and canvas errors.

use ndraw_proto::{ErrorCode, ProtocolError};
use thiserror::Error;

/// Invalid word-pool configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WordDeckError {
    /// The pool cannot satisfy one unique offer.
    #[error("word pool requires at least {required} unique words; received {actual}")]
    TooSmall {
        /// Minimum required unique words.
        required: usize,
        /// Number of supplied unique words.
        actual: usize,
    },
    /// One word is empty or otherwise invalid.
    #[error("invalid word at index {index}: {reason}")]
    InvalidWord {
        /// Zero-based input index.
        index: usize,
        /// Static validation explanation.
        reason: &'static str,
    },
    /// Two entries normalize to the same guess value.
    #[error("word at index {index} duplicates an earlier normalized word")]
    DuplicateWord {
        /// Zero-based duplicate index.
        index: usize,
    },
}

/// Failure while constructing a game.
#[derive(Debug, Error)]
pub enum GameBuildError {
    /// Room settings violate protocol constraints.
    #[error("invalid room settings: {0}")]
    InvalidSettings(#[from] ndraw_proto::ValidationError),
    /// Word-pool construction failed.
    #[error(transparent)]
    WordDeck(#[from] WordDeckError),
}

/// Invalid mutation of the authoritative canvas.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanvasError {
    /// A stroke began while another stroke was active.
    #[error("a stroke is already active")]
    StrokeAlreadyActive,
    /// A point or end operation referenced no active stroke.
    #[error("no stroke is active")]
    NoActiveStroke,
    /// A stroke identity was already used this turn.
    #[error("stroke identifier was already used")]
    DuplicateStroke,
    /// An operation references a different stroke from the active one.
    #[error("operation references the wrong active stroke")]
    WrongStroke,
    /// A point batch arrived out of sequence.
    #[error("expected sequence {expected}, received {received}")]
    WrongSequence {
        /// Next accepted sequence.
        expected: u16,
        /// Sequence supplied by the drawer.
        received: u16,
    },
    /// Sequence increment would overflow.
    #[error("stroke sequence exhausted")]
    SequenceExhausted,
    /// Protocol-level drawing validation failed.
    #[error("invalid draw operation: {0}")]
    InvalidOperation(ndraw_proto::ValidationError),
    /// Current drawing exceeded its state-memory budget.
    #[error("canvas point budget exceeded")]
    PointBudgetExceeded,
    /// Current drawing exceeded its retained action budget.
    #[error("canvas action budget exceeded")]
    ActionBudgetExceeded,
}

/// A structurally valid action rejected by authoritative game rules.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleError {
    /// Player identity is unknown to this room.
    #[error("player does not exist")]
    UnknownPlayer,
    /// Player currently has no attached connection.
    #[error("player is disconnected")]
    PlayerDisconnected,
    /// Only the current host may perform the action.
    #[error("only the host may perform this action")]
    HostOnly,
    /// Action is not valid in the current phase.
    #[error("action is not valid in the current phase")]
    InvalidPhase,
    /// Too few connected players are available to start.
    #[error("at least two connected players are required")]
    NotEnoughPlayers,
    /// Room has reached its configured capacity.
    #[error("room is full")]
    RoomFull,
    /// New identities may only join the initial lobby.
    #[error("game has already started")]
    GameAlreadyStarted,
    /// Selected word index is not in the current offer.
    #[error("word choice is out of range")]
    InvalidWordChoice,
    /// Only the current drawer may mutate the shared canvas.
    #[error("only the current drawer may draw")]
    DrawerOnly,
    /// Drawer cannot submit a guess for their own word.
    #[error("drawer cannot guess")]
    DrawerCannotGuess,
    /// Drawer cannot rate their own completed drawing.
    #[error("drawer cannot rate their own drawing")]
    DrawerCannotVote,
    /// Rating referenced a turn other than the currently displayed result.
    #[error("drawing vote references the wrong turn")]
    WrongTurn,
    /// Player has already guessed correctly this turn.
    #[error("player already guessed correctly")]
    AlreadyGuessed,
    /// Chat text would expose the current secret word.
    #[error("chat message would reveal the secret word")]
    Spoiler,
    /// Host attempted to kick their own identity.
    #[error("host cannot kick themselves")]
    CannotKickSelf,
    /// Current drawer cannot be kicked during an active turn.
    #[error("current drawer cannot be kicked")]
    CannotKickDrawer,
    /// Kick target does not exist.
    #[error("kick target does not exist")]
    InvalidKickTarget,
    /// Action arrived after the current phase deadline.
    #[error("phase deadline has elapsed")]
    DeadlineElapsed,
    /// Action payload failed protocol validation.
    #[error("invalid message: {0}")]
    InvalidMessage(ndraw_proto::ValidationError),
    /// Canvas mutation failed.
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    /// Player identifier space was exhausted.
    #[error("player identifier space exhausted")]
    PlayerIdExhausted,
    /// Turn identifier space was exhausted.
    #[error("turn identifier space exhausted")]
    TurnIdExhausted,
}

impl RuleError {
    /// Converts an internal rule failure into a user-safe wire error.
    #[must_use]
    pub fn as_protocol_error(&self) -> ProtocolError {
        let code = match self {
            Self::InvalidPhase | Self::DeadlineElapsed => ErrorCode::InvalidPhase,
            Self::HostOnly
            | Self::DrawerOnly
            | Self::DrawerCannotGuess
            | Self::DrawerCannotVote
            | Self::CannotKickSelf
            | Self::CannotKickDrawer => ErrorCode::Forbidden,
            _ => ErrorCode::InvalidMessage,
        };
        ProtocolError {
            code,
            message: self.to_string(),
        }
    }
}

/// Failure while attaching a WebSocket session to a room actor.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JoinError {
    /// New player attempted to enter after the lobby.
    #[error("game has already started")]
    GameAlreadyStarted,
    /// Room is at capacity.
    #[error("room is full")]
    RoomFull,
    /// Host previously removed this anonymous token.
    #[error("client token was kicked from this room")]
    Kicked,
    /// Profile failed protocol validation.
    #[error("invalid player profile: {0}")]
    InvalidProfile(ndraw_proto::ValidationError),
    /// Actor is shutting down or no longer reachable.
    #[error("room is closed")]
    RoomClosed,
    /// Socket writer queue was already closed or saturated during attachment.
    #[error("connection outbound queue is unavailable")]
    OutboundUnavailable,
    /// Player identifier space was exhausted.
    #[error("player identifier space exhausted")]
    PlayerIdExhausted,
}

/// Failure to enqueue a command into the bounded room mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MailboxError {
    /// Actor is still alive but its mailbox is currently full.
    #[error("room command queue is full")]
    Full,
    /// Actor has stopped and dropped its receiver.
    #[error("room is closed")]
    Closed,
}
