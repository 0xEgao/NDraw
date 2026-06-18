//! Messages emitted by the authoritative server.

use crate::{
    ids::{PlayerId, TurnId},
    model::{
        ByeReason, ChatEvent, DrawEvent, DrawingVoteUpdate, GuessResult, HintView, PhaseView,
        PlayerView, ProtocolError, Resume, ScoreView, TurnResultView, Welcome, WordOptions,
    },
};

/// A fully decoded server-to-client application message.
///
/// This enum intentionally does not implement `Serialize` or `Deserialize`.
/// The stable wire mapping is implemented explicitly in [`crate::codec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    /// New player session and its authoritative snapshot.
    Welcome(Welcome),
    /// Restored player session and its authoritative snapshot.
    Resume(Resume),
    /// A player became part of the visible roster.
    PlayerJoined(PlayerView),
    /// A player lost their active connection.
    PlayerLeft {
        /// Departed player.
        player_id: PlayerId,
    },
    /// Host authority moved to another player.
    HostChanged {
        /// New host.
        player_id: PlayerId,
    },
    /// Public game phase or deadline changed.
    PhaseChanged(PhaseView),
    /// Private candidate words for the current drawer.
    WordOptions(WordOptions),
    /// Private selected word for the current drawer.
    SecretWord {
        /// Turn for which the word is valid.
        turn_id: TurnId,
        /// Unmasked secret word.
        word: String,
    },
    /// Public word mask changed.
    HintRevealed(HintView),
    /// Authoritative drawing operation.
    Draw(DrawEvent),
    /// Accepted room chat message.
    Chat(ChatEvent),
    /// Public or private result of an evaluated guess.
    GuessResult(GuessResult),
    /// Authoritative score update.
    ScoreChanged(ScoreView),
    /// Recoverable error associated with a client action.
    Error(ProtocolError),
    /// Application heartbeat request.
    Ping {
        /// Nonce the client must echo.
        nonce: u32,
    },
    /// Final session message before socket closure.
    Bye {
        /// Reason for closure.
        reason: ByeReason,
    },
    /// Complete score breakdown for the turn that just ended.
    TurnResult(TurnResultView),
    /// Authoritative drawing-rating counts after one vote changed.
    DrawingVoteChanged(DrawingVoteUpdate),
}
