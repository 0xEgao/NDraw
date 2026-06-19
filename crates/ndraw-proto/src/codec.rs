//! Stable binary framing for WebSocket application messages.
//!
//! Every frame has the following form:
//!
//! ```text
//! byte 0      protocol version
//! byte 1      explicit direction-specific opcode
//! byte 2..N   Postcard-encoded payload
//! ```
//!
//! Message enums are deliberately not serialized as Serde enums. Their variant
//! order is a Rust implementation detail, whereas the opcode values below are
//! a public compatibility contract. Existing opcode values must never be
//! reordered or reused.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    client::ClientMessage,
    error::{DecodeError, EncodeError},
    ids::{PlayerId, TurnId},
    limit::{MAX_FRAME_BYTES, PROTOCOL_VERSION},
    model::{
        ByeReason, ChatEvent, DrawEvent, DrawOp, DrawingVote, DrawingVoteUpdate, GuessResult,
        Hello, HintView, PhaseView, PlayerView, ProtocolError, Resume, ScoreView, TurnResultView,
        Welcome, WordOptions,
    },
    server::ServerMessage,
    validate::Validate,
};

/// Frozen client-to-server opcode assignments for protocol version 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClientOpcode {
    /// [`ClientMessage::Hello`]
    Hello = 0x01,
    /// [`ClientMessage::StartGame`]
    StartGame = 0x02,
    /// [`ClientMessage::PickWord`]
    PickWord = 0x03,
    /// [`ClientMessage::Draw`]
    Draw = 0x04,
    /// [`ClientMessage::Guess`]
    Guess = 0x05,
    /// [`ClientMessage::Chat`]
    Chat = 0x06,
    /// [`ClientMessage::KickPlayer`]
    KickPlayer = 0x07,
    /// [`ClientMessage::Rematch`]
    Rematch = 0x08,
    /// [`ClientMessage::Pong`]
    Pong = 0x09,
    /// [`ClientMessage::VoteDrawing`]
    VoteDrawing = 0x0a,
}

impl ClientOpcode {
    /// Returns the stable byte written into a client frame.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for ClientOpcode {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, ()> {
        match value {
            0x01 => Ok(Self::Hello),
            0x02 => Ok(Self::StartGame),
            0x03 => Ok(Self::PickWord),
            0x04 => Ok(Self::Draw),
            0x05 => Ok(Self::Guess),
            0x06 => Ok(Self::Chat),
            0x07 => Ok(Self::KickPlayer),
            0x08 => Ok(Self::Rematch),
            0x09 => Ok(Self::Pong),
            0x0a => Ok(Self::VoteDrawing),
            _ => Err(()),
        }
    }
}

/// Frozen server-to-client opcode assignments for protocol version 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ServerOpcode {
    /// [`ServerMessage::Welcome`]
    Welcome = 0x81,
    /// [`ServerMessage::Resume`]
    Resume = 0x82,
    /// [`ServerMessage::PlayerJoined`]
    PlayerJoined = 0x83,
    /// [`ServerMessage::PlayerLeft`]
    PlayerLeft = 0x84,
    /// [`ServerMessage::HostChanged`]
    HostChanged = 0x85,
    /// [`ServerMessage::PhaseChanged`]
    PhaseChanged = 0x86,
    /// [`ServerMessage::WordOptions`]
    WordOptions = 0x87,
    /// [`ServerMessage::SecretWord`]
    SecretWord = 0x88,
    /// [`ServerMessage::HintRevealed`]
    HintRevealed = 0x89,
    /// [`ServerMessage::Draw`]
    Draw = 0x8a,
    /// [`ServerMessage::Chat`]
    Chat = 0x8b,
    /// [`ServerMessage::GuessResult`]
    GuessResult = 0x8c,
    /// [`ServerMessage::ScoreChanged`]
    ScoreChanged = 0x8d,
    /// [`ServerMessage::Error`]
    Error = 0x8e,
    /// [`ServerMessage::Ping`]
    Ping = 0x8f,
    /// [`ServerMessage::Bye`]
    Bye = 0x90,
    /// [`ServerMessage::TurnResult`]
    TurnResult = 0x91,
    /// [`ServerMessage::DrawingVoteChanged`]
    DrawingVoteChanged = 0x92,
}

impl ServerOpcode {
    /// Returns the stable byte written into a server frame.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for ServerOpcode {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, ()> {
        match value {
            0x81 => Ok(Self::Welcome),
            0x82 => Ok(Self::Resume),
            0x83 => Ok(Self::PlayerJoined),
            0x84 => Ok(Self::PlayerLeft),
            0x85 => Ok(Self::HostChanged),
            0x86 => Ok(Self::PhaseChanged),
            0x87 => Ok(Self::WordOptions),
            0x88 => Ok(Self::SecretWord),
            0x89 => Ok(Self::HintRevealed),
            0x8a => Ok(Self::Draw),
            0x8b => Ok(Self::Chat),
            0x8c => Ok(Self::GuessResult),
            0x8d => Ok(Self::ScoreChanged),
            0x8e => Ok(Self::Error),
            0x8f => Ok(Self::Ping),
            0x90 => Ok(Self::Bye),
            0x91 => Ok(Self::TurnResult),
            0x92 => Ok(Self::DrawingVoteChanged),
            _ => Err(()),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SecretWordPayload {
    turn_id: TurnId,
    word: String,
}

#[derive(Serialize, Deserialize)]
struct VoteDrawingPayload {
    turn_id: TurnId,
    vote: Option<DrawingVote>,
}

fn payload_bytes<T>(payload: &T) -> Result<Vec<u8>, EncodeError>
where
    T: Serialize + ?Sized,
{
    postcard::to_stdvec(payload).map_err(EncodeError::from)
}

fn make_frame(opcode: u8, payload: Vec<u8>) -> Result<Vec<u8>, EncodeError> {
    let actual = payload.len().saturating_add(2);
    if actual > MAX_FRAME_BYTES {
        return Err(EncodeError::FrameTooLarge {
            maximum: MAX_FRAME_BYTES,
            actual,
        });
    }

    let mut frame = Vec::with_capacity(actual);
    frame.push(PROTOCOL_VERSION);
    frame.push(opcode);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn parse_header<'frame>(
    frame: &'frame [u8],
    direction: &'static str,
) -> Result<(u8, &'frame [u8]), DecodeError> {
    if frame.len() < 2 {
        return Err(DecodeError::FrameTooShort {
            actual: frame.len(),
        });
    }
    if frame.len() > MAX_FRAME_BYTES {
        return Err(DecodeError::FrameTooLarge {
            maximum: MAX_FRAME_BYTES,
            actual: frame.len(),
        });
    }
    if frame[0] != PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            expected: PROTOCOL_VERSION,
            received: frame[0],
        });
    }

    let opcode = frame[1];
    if direction == "client" {
        ClientOpcode::try_from(opcode)
            .map_err(|()| DecodeError::UnknownOpcode { direction, opcode })?;
    } else {
        ServerOpcode::try_from(opcode)
            .map_err(|()| DecodeError::UnknownOpcode { direction, opcode })?;
    }

    Ok((opcode, &frame[2..]))
}

fn decode_payload<T>(payload: &[u8]) -> Result<T, DecodeError>
where
    T: DeserializeOwned,
{
    let (value, remaining) = postcard::take_from_bytes(payload)?;
    if !remaining.is_empty() {
        return Err(DecodeError::TrailingBytes {
            remaining: remaining.len(),
        });
    }
    Ok(value)
}

fn require_empty_payload(payload: &[u8]) -> Result<(), DecodeError> {
    if payload.is_empty() {
        Ok(())
    } else {
        Err(DecodeError::TrailingBytes {
            remaining: payload.len(),
        })
    }
}

/// Encodes and validates one client-to-server message.
pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, EncodeError> {
    message.validate()?;

    let (opcode, payload) = match message {
        ClientMessage::Hello(value) => (ClientOpcode::Hello, payload_bytes(value)?),
        ClientMessage::StartGame => (ClientOpcode::StartGame, Vec::new()),
        ClientMessage::PickWord { choice } => (ClientOpcode::PickWord, payload_bytes(choice)?),
        ClientMessage::Draw(value) => (ClientOpcode::Draw, payload_bytes(value)?),
        ClientMessage::Guess { text } => (ClientOpcode::Guess, payload_bytes(text)?),
        ClientMessage::Chat { text } => (ClientOpcode::Chat, payload_bytes(text)?),
        ClientMessage::KickPlayer { player_id } => {
            (ClientOpcode::KickPlayer, payload_bytes(player_id)?)
        }
        ClientMessage::Rematch => (ClientOpcode::Rematch, Vec::new()),
        ClientMessage::Pong { nonce } => (ClientOpcode::Pong, payload_bytes(nonce)?),
        ClientMessage::VoteDrawing { turn_id, vote } => (
            ClientOpcode::VoteDrawing,
            payload_bytes(&VoteDrawingPayload {
                turn_id: *turn_id,
                vote: *vote,
            })?,
        ),
    };

    make_frame(opcode.as_u8(), payload)
}

/// Decodes and validates one client-to-server frame.
pub fn decode_client(frame: &[u8]) -> Result<ClientMessage, DecodeError> {
    let (opcode, payload) = parse_header(frame, "client")?;
    let message = match ClientOpcode::try_from(opcode) {
        Ok(ClientOpcode::Hello) => ClientMessage::Hello(decode_payload::<Hello>(payload)?),
        Ok(ClientOpcode::StartGame) => {
            require_empty_payload(payload)?;
            ClientMessage::StartGame
        }
        Ok(ClientOpcode::PickWord) => ClientMessage::PickWord {
            choice: decode_payload::<u8>(payload)?,
        },
        Ok(ClientOpcode::Draw) => ClientMessage::Draw(decode_payload::<DrawOp>(payload)?),
        Ok(ClientOpcode::Guess) => ClientMessage::Guess {
            text: decode_payload::<String>(payload)?,
        },
        Ok(ClientOpcode::Chat) => ClientMessage::Chat {
            text: decode_payload::<String>(payload)?,
        },
        Ok(ClientOpcode::KickPlayer) => ClientMessage::KickPlayer {
            player_id: decode_payload::<PlayerId>(payload)?,
        },
        Ok(ClientOpcode::Rematch) => {
            require_empty_payload(payload)?;
            ClientMessage::Rematch
        }
        Ok(ClientOpcode::Pong) => ClientMessage::Pong {
            nonce: decode_payload::<u32>(payload)?,
        },
        Ok(ClientOpcode::VoteDrawing) => {
            let value = decode_payload::<VoteDrawingPayload>(payload)?;
            ClientMessage::VoteDrawing {
                turn_id: value.turn_id,
                vote: value.vote,
            }
        }
        Err(()) => {
            return Err(DecodeError::UnknownOpcode {
                direction: "client",
                opcode,
            });
        }
    };

    message.validate()?;
    Ok(message)
}

/// Encodes and validates one server-to-client message.
pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, EncodeError> {
    message.validate()?;

    let (opcode, payload) = match message {
        ServerMessage::Welcome(value) => (ServerOpcode::Welcome, payload_bytes(value)?),
        ServerMessage::Resume(value) => (ServerOpcode::Resume, payload_bytes(value)?),
        ServerMessage::PlayerJoined(value) => (ServerOpcode::PlayerJoined, payload_bytes(value)?),
        ServerMessage::PlayerLeft { player_id } => {
            (ServerOpcode::PlayerLeft, payload_bytes(player_id)?)
        }
        ServerMessage::HostChanged { player_id } => {
            (ServerOpcode::HostChanged, payload_bytes(player_id)?)
        }
        ServerMessage::PhaseChanged(value) => (ServerOpcode::PhaseChanged, payload_bytes(value)?),
        ServerMessage::WordOptions(value) => (ServerOpcode::WordOptions, payload_bytes(value)?),
        ServerMessage::SecretWord { turn_id, word } => (
            ServerOpcode::SecretWord,
            payload_bytes(&SecretWordPayload {
                turn_id: *turn_id,
                word: word.clone(),
            })?,
        ),
        ServerMessage::HintRevealed(value) => (ServerOpcode::HintRevealed, payload_bytes(value)?),
        ServerMessage::Draw(value) => (ServerOpcode::Draw, payload_bytes(value)?),
        ServerMessage::Chat(value) => (ServerOpcode::Chat, payload_bytes(value)?),
        ServerMessage::GuessResult(value) => (ServerOpcode::GuessResult, payload_bytes(value)?),
        ServerMessage::ScoreChanged(value) => (ServerOpcode::ScoreChanged, payload_bytes(value)?),
        ServerMessage::Error(value) => (ServerOpcode::Error, payload_bytes(value)?),
        ServerMessage::Ping { nonce } => (ServerOpcode::Ping, payload_bytes(nonce)?),
        ServerMessage::Bye { reason } => (ServerOpcode::Bye, payload_bytes(reason)?),
        ServerMessage::TurnResult(value) => (ServerOpcode::TurnResult, payload_bytes(value)?),
        ServerMessage::DrawingVoteChanged(value) => {
            (ServerOpcode::DrawingVoteChanged, payload_bytes(value)?)
        }
    };

    make_frame(opcode.as_u8(), payload)
}

/// Decodes and validates one server-to-client frame.
pub fn decode_server(frame: &[u8]) -> Result<ServerMessage, DecodeError> {
    let (opcode, payload) = parse_header(frame, "server")?;
    let message = match ServerOpcode::try_from(opcode) {
        Ok(ServerOpcode::Welcome) => ServerMessage::Welcome(decode_payload::<Welcome>(payload)?),
        Ok(ServerOpcode::Resume) => ServerMessage::Resume(decode_payload::<Resume>(payload)?),
        Ok(ServerOpcode::PlayerJoined) => {
            ServerMessage::PlayerJoined(decode_payload::<PlayerView>(payload)?)
        }
        Ok(ServerOpcode::PlayerLeft) => ServerMessage::PlayerLeft {
            player_id: decode_payload::<PlayerId>(payload)?,
        },
        Ok(ServerOpcode::HostChanged) => ServerMessage::HostChanged {
            player_id: decode_payload::<PlayerId>(payload)?,
        },
        Ok(ServerOpcode::PhaseChanged) => {
            ServerMessage::PhaseChanged(decode_payload::<PhaseView>(payload)?)
        }
        Ok(ServerOpcode::WordOptions) => {
            ServerMessage::WordOptions(decode_payload::<WordOptions>(payload)?)
        }
        Ok(ServerOpcode::SecretWord) => {
            let value = decode_payload::<SecretWordPayload>(payload)?;
            ServerMessage::SecretWord {
                turn_id: value.turn_id,
                word: value.word,
            }
        }
        Ok(ServerOpcode::HintRevealed) => {
            ServerMessage::HintRevealed(decode_payload::<HintView>(payload)?)
        }
        Ok(ServerOpcode::Draw) => ServerMessage::Draw(decode_payload::<DrawEvent>(payload)?),
        Ok(ServerOpcode::Chat) => ServerMessage::Chat(decode_payload::<ChatEvent>(payload)?),
        Ok(ServerOpcode::GuessResult) => {
            ServerMessage::GuessResult(decode_payload::<GuessResult>(payload)?)
        }
        Ok(ServerOpcode::ScoreChanged) => {
            ServerMessage::ScoreChanged(decode_payload::<ScoreView>(payload)?)
        }
        Ok(ServerOpcode::Error) => ServerMessage::Error(decode_payload::<ProtocolError>(payload)?),
        Ok(ServerOpcode::Ping) => ServerMessage::Ping {
            nonce: decode_payload::<u32>(payload)?,
        },
        Ok(ServerOpcode::Bye) => ServerMessage::Bye {
            reason: decode_payload::<ByeReason>(payload)?,
        },
        Ok(ServerOpcode::TurnResult) => {
            ServerMessage::TurnResult(decode_payload::<TurnResultView>(payload)?)
        }
        Ok(ServerOpcode::DrawingVoteChanged) => {
            ServerMessage::DrawingVoteChanged(decode_payload::<DrawingVoteUpdate>(payload)?)
        }
        Err(()) => {
            return Err(DecodeError::UnknownOpcode {
                direction: "server",
                opcode,
            });
        }
    };

    message.validate()?;
    Ok(message)
}
