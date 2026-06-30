//! Authoritative room and game engine for NDraw.
//!
//! `ndraw-den` contains two deliberately separate layers:
//!
//! - [`game::Game`] is a synchronous, deterministic state machine. Callers
//!   provide time explicitly, making every rule testable without sleeping.
//! - [`actor`] wraps one game in a single Tokio task and owns room sessions,
//!   reconnect generations, bounded fanout, and lifecycle deadlines.
//!
//! Neither layer knows about Axum, WebSockets, HTTP, Redis, or persistence.

#![forbid(unsafe_code)]

pub mod actor;
pub mod canvas;
pub mod error;
pub mod game;
pub mod guess;
pub mod session;
pub mod time;
pub mod words;

pub use actor::{
    JoinAccepted, JoinRequest, RoomConfig, RoomExit, RoomExitReason, RoomHandle, RoomTask,
    spawn_room,
};
pub use canvas::{ActiveStroke, CanvasState};
pub use error::{CanvasError, GameBuildError, JoinError, MailboxError, RuleError, WordDeckError};
pub use game::{Audience, EmittedEvent, Game, GameStateView, PlayerAction, PlayerState};
pub use session::ConnectionLease;
pub use time::{GameDeadline, GameTime};
pub use words::WordDeck;
