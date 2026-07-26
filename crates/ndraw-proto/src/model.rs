//! Data structures shared by client and server messages.

use serde::{Deserialize, Serialize};

use crate::ids::{ClientToken, PlayerId, RoomCode, StrokeId, TurnId};

/// User-controlled identity displayed to other room members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerProfile {
    /// Display name shown in the lobby, chat, and scoreboard.
    pub display_name: String,
    /// Opaque client-owned avatar configuration.
    pub avatar: [u8; 8],
}

/// Validated gameplay configuration chosen when a room is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSettings {
    /// Number of complete passes through the player rotation.
    pub rounds: u8,
    /// Drawing duration for each turn.
    pub draw_seconds: u16,
    /// Number of candidate words offered to a drawer.
    pub word_choices: u8,
    /// Maximum number of players allowed in this room.
    pub max_players: u8,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            rounds: 3,
            draw_seconds: 100,
            word_choices: 4,
            max_players: 12,
        }
    }
}

/// A point in the fixed logical canvas coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal logical coordinate.
    pub x: u16,
    /// Vertical logical coordinate.
    pub y: u16,
}

/// One client request that mutates the current drawing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawOp {
    /// Starts a new stroke.
    Begin {
        /// Identity unique within the current turn.
        stroke_id: StrokeId,
        /// RGB color encoded as `0x00RRGGBB`.
        color: u32,
        /// Brush width in logical canvas units.
        width: u8,
        /// Initial stroke point.
        start: Point,
    },
    /// Appends an ordered point batch to the active stroke.
    Points {
        /// Stroke receiving the points.
        stroke_id: StrokeId,
        /// Monotonically increasing batch sequence number.
        sequence: u16,
        /// Non-empty point batch.
        points: Vec<Point>,
    },
    /// Completes an active stroke.
    End {
        /// Stroke being completed.
        stroke_id: StrokeId,
        /// Sequence number following the final points batch.
        sequence: u16,
    },
    /// Removes the most recently completed stroke.
    Undo,
    /// Removes every stroke from the current canvas.
    Clear,
    /// Flood-fills the contiguous color region containing `at`.
    Fill {
        /// RGB replacement color encoded as `0x00RRGGBB`.
        color: u32,
        /// Seed point whose current pixel color identifies the filled region.
        at: Point,
    },
}

/// Complete stroke representation used in reconnect snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stroke {
    /// Stroke identity within its turn.
    pub stroke_id: StrokeId,
    /// RGB color encoded as `0x00RRGGBB`.
    pub color: u32,
    /// Brush width in logical canvas units.
    pub width: u8,
    /// Ordered stroke points, including its starting point.
    pub points: Vec<Point>,
}

/// One completed canvas mutation retained in authoritative rendering order.
///
/// Keeping fills interleaved with strokes is important: replaying every stroke
/// before every fill would let later shape boundaries incorrectly affect an
/// earlier fill after a reconnect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanvasAction {
    /// One completed or currently active stroke.
    Stroke(Stroke),
    /// A point-seeded contiguous-region fill.
    Fill {
        /// RGB replacement color encoded as `0x00RRGGBB`.
        color: u32,
        /// Logical seed point used by clients when replaying the fill.
        at: Point,
    },
}

/// Current canvas contents sent when a player joins or resumes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasSnapshot {
    /// Completed strokes and fills in authoritative rendering order.
    pub actions: Vec<CanvasAction>,
}

/// First application message sent by a newly upgraded client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Browser-persisted anonymous identity.
    pub client_token: ClientToken,
    /// User-visible profile for this room.
    pub profile: PlayerProfile,
}

/// Public information about one player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    /// Server-assigned player identity.
    pub player_id: PlayerId,
    /// User-visible profile.
    pub profile: PlayerProfile,
    /// Accumulated game score.
    pub score: u32,
    /// Whether this player currently has an attached connection.
    pub connected: bool,
    /// Whether this player currently owns host controls.
    pub is_host: bool,
    /// Whether this player has guessed the current word.
    pub has_guessed: bool,
}

/// Public game phase visible to clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    /// Players are gathering and the host may start the game.
    Lobby,
    /// The drawer is privately choosing a word.
    ChoosingWord,
    /// A drawing and guessing turn is active.
    Drawing,
    /// Scores for the previous turn are being displayed.
    RoundEnd,
    /// The configured number of rounds has completed.
    GameOver,
}

/// Public state associated with the current phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseView {
    /// Current phase.
    pub phase: GamePhase,
    /// One-based round number, or zero while still in the initial lobby.
    pub round: u8,
    /// Configured total number of rounds.
    pub total_rounds: u8,
    /// Current drawer when a turn exists.
    pub drawer: Option<PlayerId>,
    /// Current turn identity when a turn exists.
    pub turn_id: Option<TurnId>,
    /// Server-authoritative deadline represented as Unix milliseconds.
    pub deadline_unix_ms: Option<u64>,
    /// Partially revealed word displayed to guessers.
    pub masked_word: Option<String>,
}

/// Candidate words sent only to the current drawer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordOptions {
    /// Turn for which the choices are valid.
    pub turn_id: TurnId,
    /// Ordered candidate words. The client replies with an index.
    pub words: Vec<String>,
}

/// Newly revealed public word mask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HintView {
    /// Turn receiving the hint.
    pub turn_id: TurnId,
    /// Word with unrevealed positions replaced by client-display placeholders.
    pub masked_word: String,
}

/// Authoritative drawing operation rebroadcast by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawEvent {
    /// Drawer who produced the operation.
    pub player_id: PlayerId,
    /// Turn to which the operation belongs.
    pub turn_id: TurnId,
    /// Validated canvas mutation.
    pub operation: DrawOp,
}

/// Chat message accepted by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEvent {
    /// Message author.
    pub player_id: PlayerId,
    /// Whether the submitted text was ordinary chat or a non-winning guess.
    pub kind: ChatKind,
    /// Plain-text message. Clients must still render it as text, never HTML.
    pub text: String,
}

/// Public text category displayed in the shared chat timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatKind {
    /// Explicit chat sent from the lobby, drawer, or a player who already guessed.
    Chat,
    /// An incorrect or close guess safe to reveal to all participants.
    Guess,
}

/// Result of evaluating a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuessOutcome {
    /// Guess does not sufficiently resemble the secret word.
    Incorrect,
    /// Guess is close enough for a private hint but is not correct.
    Close,
    /// Guess is correct and earned the included number of points.
    Correct {
        /// Points awarded for this guess.
        points: u32,
    },
}

/// Server response describing one evaluated guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuessResult {
    /// Player whose guess was evaluated.
    pub player_id: PlayerId,
    /// Authoritative outcome.
    pub outcome: GuessOutcome,
}

/// One authoritative scoreboard update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreView {
    /// Player whose score changed.
    pub player_id: PlayerId,
    /// New total score.
    pub total_score: u32,
    /// Signed delta applied by this update.
    pub delta: i32,
}

/// Why a player received points during the completed turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AwardReason {
    /// The player correctly guessed the secret word.
    CorrectGuess,
    /// The drawer received one quarter of the turn's combined guesser points.
    DrawerBonus,
}

/// One score contribution shown in the completed-turn summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnAward {
    /// Player who received the points.
    pub player_id: PlayerId,
    /// Points added during this turn.
    pub points: u32,
    /// Server-authoritative reason for the award.
    pub reason: AwardReason,
}

/// Authoritative summary retained while a completed drawing is displayed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnResultView {
    /// Completed turn identity.
    pub turn_id: TurnId,
    /// Revealed word for the completed drawing.
    pub word: String,
    /// Awards in deterministic room-player order.
    pub awards: Vec<TurnAward>,
}

/// Positive or negative feedback on a completed drawing.
///
/// Votes are social feedback only and never change game scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawingVote {
    /// Viewer liked the completed drawing.
    Like,
    /// Viewer disliked the completed drawing.
    Dislike,
}

/// Viewer-specific rating state included in reconnect snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingRatingView {
    /// Completed turn identity.
    pub turn_id: TurnId,
    /// Current number of likes.
    pub likes: u8,
    /// Current number of dislikes.
    pub dislikes: u8,
    /// This snapshot viewer's current vote, if any.
    pub viewer_vote: Option<DrawingVote>,
}

/// Public update emitted after one player changes their drawing vote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingVoteUpdate {
    /// Completed turn identity.
    pub turn_id: TurnId,
    /// Player whose vote changed.
    pub player_id: PlayerId,
    /// New vote, or `None` when the player removed it.
    pub vote: Option<DrawingVote>,
    /// Authoritative like count after the change.
    pub likes: u8,
    /// Authoritative dislike count after the change.
    pub dislikes: u8,
}

/// Stable machine-readable server error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// Message is not legal in the current game phase.
    InvalidPhase,
    /// Player does not have permission to perform the action.
    Forbidden,
    /// Message contains an invalid value.
    InvalidMessage,
    /// Per-connection rate limit was exceeded.
    RateLimited,
    /// Room command queue is temporarily saturated.
    ServerBusy,
}

/// Non-fatal application error returned to one client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    /// Stable category for programmatic handling.
    pub code: ErrorCode,
    /// Short user-safe explanation.
    pub message: String,
}

/// Reason the server is ending a client session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ByeReason {
    /// Room actor has completed its lifecycle.
    RoomClosed,
    /// Host removed this player.
    Kicked,
    /// A newer connection reused the same client token.
    Replaced,
    /// Peer failed the application heartbeat.
    TimedOut,
    /// Server is shutting down gracefully.
    ServerShutdown,
    /// Peer sent an unrecoverable protocol violation.
    ProtocolViolation,
}

/// Complete viewer-specific room state used by welcome and resume messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSnapshot {
    /// Immutable room configuration.
    pub settings: RoomSettings,
    /// Currently known players in deterministic display order.
    pub players: Vec<PlayerView>,
    /// Current public phase state.
    pub phase: PhaseView,
    /// Current drawing reconstructed for a joining client.
    pub canvas: CanvasSnapshot,
    /// Private options included only when the viewer is choosing a word.
    pub word_options: Option<WordOptions>,
    /// Private word included only when the viewer is the current drawer.
    pub secret_word: Option<String>,
    /// Bounded recent chat backlog.
    pub chat_history: Vec<ChatEvent>,
    /// Completed-turn score breakdown, present only during round end.
    pub turn_result: Option<TurnResultView>,
    /// Completed drawing rating, including this viewer's vote.
    pub drawing_rating: Option<DrawingRatingView>,
}

/// Initial session acknowledgement for a new player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    /// Identity assigned to this connection.
    pub player_id: PlayerId,
    /// Room joined by this connection.
    pub room_code: RoomCode,
    /// Viewer-specific authoritative room state.
    pub snapshot: RoomSnapshot,
}

/// Session acknowledgement when reclaiming an existing player slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resume {
    /// Reclaimed player identity.
    pub player_id: PlayerId,
    /// Viewer-specific authoritative room state.
    pub snapshot: RoomSnapshot,
}
