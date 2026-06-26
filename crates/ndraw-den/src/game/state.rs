//! Internal phase state and read-only diagnostics.

use std::collections::{HashMap, HashSet};

use ndraw_proto::{DrawingVote, GamePhase, PlayerId, PlayerProfile, TurnAward, TurnId};

use crate::{canvas::CanvasState, time::GameDeadline};

/// Authoritative mutable player record retained across reconnects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerState {
    /// Room-scoped identity.
    pub player_id: PlayerId,
    /// User-visible profile.
    pub profile: PlayerProfile,
    /// Accumulated game score.
    pub score: u32,
    /// Whether a current connection generation exists.
    pub connected: bool,
    /// Monotonic insertion order used for host transfer.
    pub joined_order: u64,
}

#[derive(Debug)]
pub(super) struct LobbyState {
    pub deadline: GameDeadline,
}

#[derive(Debug)]
pub(super) struct ChoosingWordState {
    pub drawer: PlayerId,
    pub turn_id: TurnId,
    pub options: Vec<String>,
    pub deadline: GameDeadline,
}

#[derive(Debug)]
pub(super) struct DrawingState {
    pub drawer: PlayerId,
    pub turn_id: TurnId,
    pub secret_word: String,
    pub word_characters: Vec<char>,
    pub revealed: Vec<bool>,
    pub guessed: HashSet<PlayerId>,
    pub guess_awards: HashMap<PlayerId, u32>,
    pub guesser_score_total: u32,
    pub canvas: CanvasState,
    pub deadline: GameDeadline,
    pub hint_deadlines: Vec<GameDeadline>,
    pub next_hint: usize,
}

#[derive(Debug)]
pub(super) struct RoundEndState {
    pub drawer: PlayerId,
    pub turn_id: TurnId,
    pub secret_word: String,
    pub canvas: CanvasState,
    pub awards: Vec<TurnAward>,
    pub votes: HashMap<PlayerId, DrawingVote>,
    pub deadline: GameDeadline,
}

#[derive(Debug)]
pub(super) enum PhaseState {
    Lobby(LobbyState),
    ChoosingWord(ChoosingWordState),
    Drawing(DrawingState),
    RoundEnd(RoundEndState),
    GameOver,
}

impl PhaseState {
    pub fn public_phase(&self) -> GamePhase {
        match self {
            Self::Lobby(_) => GamePhase::Lobby,
            Self::ChoosingWord(_) => GamePhase::ChoosingWord,
            Self::Drawing(_) => GamePhase::Drawing,
            Self::RoundEnd(_) => GamePhase::RoundEnd,
            Self::GameOver => GamePhase::GameOver,
        }
    }
}

/// Compact immutable view intended for tests, diagnostics, and metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameStateView {
    /// Current public phase.
    pub phase: GamePhase,
    /// Current one-based round, or zero in the lobby.
    pub round: u8,
    /// Current host, if one has joined.
    pub host: Option<PlayerId>,
    /// Current drawer, if a turn exists.
    pub drawer: Option<PlayerId>,
    /// Current turn identity, if a turn exists.
    pub turn_id: Option<TurnId>,
    /// Players with active connections.
    pub connected_players: usize,
    /// Scores in deterministic player order.
    pub scores: Vec<(PlayerId, u32)>,
}
