//! Limits that are part of protocol version 2.
//!
//! Both producers and consumers enforce these values. Keeping them in the
//! protocol crate gives servers, load generators, and native clients the same
//! definition of a valid message.

/// Version byte placed at the beginning of every binary frame.
pub const PROTOCOL_VERSION: u8 = 2;

/// Largest accepted binary WebSocket message, including version and opcode.
pub const MAX_FRAME_BYTES: usize = 16 * 1024;

/// Largest number of players that a room may advertise.
pub const MAX_PLAYERS_IN_ROOM: usize = 12;

/// Largest display name measured in Unicode grapheme clusters.
pub const MAX_NAME_GRAPHEMES: usize = 24;

/// Largest chat message or guess measured as UTF-8 bytes.
pub const MAX_TEXT_BYTES: usize = 200;

/// Largest word or masked-word representation measured as UTF-8 bytes.
pub const MAX_WORD_BYTES: usize = 96;

/// Largest number of points accepted in one drawing batch.
pub const MAX_POINTS_PER_BATCH: usize = 64;

/// Maximum number of ordered strokes and fills carried in a reconnect snapshot.
pub const MAX_CANVAS_ACTIONS: usize = 2_048;

/// Maximum number of points carried in one complete stroke snapshot.
pub const MAX_POINTS_PER_STROKE: usize = 16_384;

/// Logical canvas width. Valid horizontal coordinates are `0..=CANVAS_WIDTH`.
pub const CANVAS_WIDTH: u16 = 1_024;

/// Logical canvas height. Valid vertical coordinates are `0..=CANVAS_HEIGHT`.
pub const CANVAS_HEIGHT: u16 = 960;

/// Largest chat backlog included in a room snapshot.
pub const MAX_CHAT_HISTORY: usize = 64;

/// Number of ASCII characters in a room code.
pub const ROOM_CODE_LENGTH: usize = 6;

/// Alphabet used by room-code generators.
///
/// Easily confused characters (`0`, `1`, `I`, and `O`) are intentionally
/// omitted.
pub const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Minimum supported number of rounds.
pub const MIN_ROUNDS: u8 = 1;

/// Maximum supported number of rounds.
pub const MAX_ROUNDS: u8 = 10;

/// Minimum drawing duration in seconds.
pub const MIN_DRAW_SECONDS: u16 = 15;

/// Maximum drawing duration in seconds.
pub const MAX_DRAW_SECONDS: u16 = 300;

/// Minimum number of word choices offered to a drawer.
pub const MIN_WORD_CHOICES: u8 = 1;

/// Maximum number of word choices offered to a drawer.
pub const MAX_WORD_CHOICES: u8 = 8;

/// Minimum number of players required by a room configuration.
pub const MIN_ROOM_PLAYERS: u8 = 2;

/// Largest supported brush width in logical canvas units.
pub const MAX_BRUSH_WIDTH: u8 = 64;
