//! Versioned binary protocol for NDraw.
//!
//! This crate contains only transport-safe data, validation, and binary codec
//! logic. It deliberately contains no game-state authority, socket handling,
//! timers, or persistence. The server, load generator, and non-web clients can
//! therefore share one exact definition of the wire.
//!
//! # Wire compatibility
//!
//! Frames begin with a protocol version and an explicit opcode. The remaining
//! payload uses Postcard's stable format. Existing opcode values and payload
//! field order are compatibility contracts for protocol version 2.
//!
//! # Safety
//!
//! Decode functions enforce the frame-size limit before deserializing and
//! validate all decoded allocations before returning them to callers. Stateful
//! authorization, such as whether a player may draw, belongs in `ndraw-den`.

#![forbid(unsafe_code)]

pub mod client;
pub mod codec;
pub mod error;
pub mod ids;
pub mod limit;
pub mod model;
pub mod server;
pub mod validate;

pub use client::ClientMessage;
pub use codec::{
    ClientOpcode, ServerOpcode, decode_client, decode_server, encode_client, encode_server,
};
pub use error::{DecodeError, EncodeError, RoomCodeParseError, ValidationError};
pub use ids::{ClientToken, PlayerId, RoomCode, StrokeId, TurnId};
pub use model::*;
pub use server::ServerMessage;
pub use validate::Validate;
