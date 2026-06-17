//! Strongly typed identifiers shared across the wire.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::RoomCodeParseError,
    limit::{ROOM_CODE_ALPHABET, ROOM_CODE_LENGTH},
};

/// Stable anonymous browser identity used to reclaim a departed player slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientToken(pub Uuid);

impl ClientToken {
    /// Creates a cryptographically random version-4 client token.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ClientToken {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClientToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ClientToken {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// Server-assigned player identity within one room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub u32);

impl fmt::Display for PlayerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Monotonically increasing drawing-turn identity within one room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TurnId(pub u32);

impl fmt::Display for TurnId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Drawer-assigned stroke identity, scoped to one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrokeId(pub u32);

impl fmt::Display for StrokeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Six-character, human-shareable room identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RoomCode([u8; ROOM_CODE_LENGTH]);

impl RoomCode {
    /// Validates and constructs a room code from its ASCII bytes.
    pub fn new(bytes: [u8; ROOM_CODE_LENGTH]) -> Result<Self, RoomCodeParseError> {
        if let Some(index) = bytes
            .iter()
            .position(|byte| !ROOM_CODE_ALPHABET.contains(byte))
        {
            return Err(RoomCodeParseError::InvalidCharacter { index });
        }

        Ok(Self(bytes))
    }

    /// Returns the validated ASCII bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; ROOM_CODE_LENGTH] {
        &self.0
    }
}

impl fmt::Display for RoomCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            formatter.write_fmt(format_args!("{}", char::from(byte)))?;
        }
        Ok(())
    }
}

impl FromStr for RoomCode {
    type Err = RoomCodeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes: [u8; ROOM_CODE_LENGTH] =
            value
                .as_bytes()
                .try_into()
                .map_err(
                    |_: std::array::TryFromSliceError| RoomCodeParseError::InvalidLength {
                        expected: ROOM_CODE_LENGTH,
                        actual: value.len(),
                    },
                )?;

        Self::new(bytes)
    }
}

impl TryFrom<String> for RoomCode {
    type Error = RoomCodeParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<RoomCode> for String {
    fn from(value: RoomCode) -> Self {
        value.to_string()
    }
}
