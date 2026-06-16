//! Errors returned by protocol parsing, validation, and encoding.

use thiserror::Error;

/// A semantic constraint violation in an otherwise decodable value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    /// A required string contains no user-visible content.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the invalid field.
        field: &'static str,
    },

    /// A string or collection exceeds its protocol limit.
    #[error("{field} exceeds its maximum of {maximum}; received {actual}")]
    TooLong {
        /// Name of the invalid field.
        field: &'static str,
        /// Maximum accepted length.
        maximum: usize,
        /// Observed length.
        actual: usize,
    },

    /// A collection contains fewer values than required.
    #[error("{field} requires at least {minimum} item(s); received {actual}")]
    TooShort {
        /// Name of the invalid field.
        field: &'static str,
        /// Minimum accepted length.
        minimum: usize,
        /// Observed length.
        actual: usize,
    },

    /// A numeric value falls outside its inclusive range.
    #[error("{field} must be in {minimum}..={maximum}; received {actual}")]
    OutOfRange {
        /// Name of the invalid field.
        field: &'static str,
        /// Inclusive lower bound.
        minimum: u64,
        /// Inclusive upper bound.
        maximum: u64,
        /// Observed value.
        actual: u64,
    },

    /// A string contains a character forbidden by the protocol.
    #[error("{field} contains a forbidden control character")]
    ControlCharacter {
        /// Name of the invalid field.
        field: &'static str,
    },

    /// A message contradicts another value in the same payload.
    #[error("{field} is inconsistent: {reason}")]
    Inconsistent {
        /// Name of the invalid field.
        field: &'static str,
        /// Static explanation suitable for logs and tests.
        reason: &'static str,
    },
}

/// Failure while converting user input into a room code.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoomCodeParseError {
    /// The code does not contain exactly six bytes.
    #[error("room code must contain exactly {expected} ASCII characters; received {actual}")]
    InvalidLength {
        /// Required room-code length.
        expected: usize,
        /// Observed byte length.
        actual: usize,
    },

    /// The code contains a character outside [`crate::limit::ROOM_CODE_ALPHABET`].
    #[error("room code contains an invalid character at byte {index}")]
    InvalidCharacter {
        /// Zero-based byte position of the invalid value.
        index: usize,
    },
}

/// Failure while encoding an outgoing protocol frame.
#[derive(Debug, Error)]
pub enum EncodeError {
    /// The message violates a protocol constraint.
    #[error(transparent)]
    Validation(#[from] ValidationError),

    /// Postcard could not encode the selected payload.
    #[error("failed to encode postcard payload: {0}")]
    Postcard(#[from] postcard::Error),

    /// The final frame exceeds [`crate::limit::MAX_FRAME_BYTES`].
    #[error("encoded frame exceeds {maximum} bytes; produced {actual}")]
    FrameTooLarge {
        /// Maximum accepted frame size.
        maximum: usize,
        /// Encoded frame size.
        actual: usize,
    },
}

/// Failure while decoding an incoming protocol frame.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// A frame is missing its version or opcode byte.
    #[error("frame is too short: expected at least 2 bytes, received {actual}")]
    FrameTooShort {
        /// Observed frame size.
        actual: usize,
    },

    /// A frame exceeds the hard allocation and processing limit.
    #[error("frame exceeds {maximum} bytes; received {actual}")]
    FrameTooLarge {
        /// Maximum accepted frame size.
        maximum: usize,
        /// Observed frame size.
        actual: usize,
    },

    /// The peer speaks a different protocol version.
    #[error("unsupported protocol version {received}; expected {expected}")]
    UnsupportedVersion {
        /// Version supported by this crate.
        expected: u8,
        /// Version found in the frame.
        received: u8,
    },

    /// The opcode is not assigned for the frame direction.
    #[error("unknown {direction} opcode 0x{opcode:02x}")]
    UnknownOpcode {
        /// Human-readable frame direction.
        direction: &'static str,
        /// Unrecognized opcode byte.
        opcode: u8,
    },

    /// Postcard could not decode the selected payload.
    #[error("failed to decode postcard payload: {0}")]
    Postcard(#[from] postcard::Error),

    /// Bytes remain after the expected payload.
    #[error("frame contains {remaining} trailing byte(s)")]
    TrailingBytes {
        /// Number of unconsumed bytes.
        remaining: usize,
    },

    /// The decoded message violates a protocol constraint.
    #[error(transparent)]
    Validation(#[from] ValidationError),
}
