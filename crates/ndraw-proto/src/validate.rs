//! Semantic validation for decoded and outgoing protocol values.

use std::collections::HashSet;

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    client::ClientMessage,
    error::ValidationError,
    limit::{
        CANVAS_HEIGHT, CANVAS_WIDTH, MAX_BRUSH_WIDTH, MAX_CANVAS_ACTIONS, MAX_CHAT_HISTORY,
        MAX_DRAW_SECONDS, MAX_NAME_GRAPHEMES, MAX_PLAYERS_IN_ROOM, MAX_POINTS_PER_BATCH,
        MAX_POINTS_PER_STROKE, MAX_ROUNDS, MAX_TEXT_BYTES, MAX_WORD_BYTES, MAX_WORD_CHOICES,
        MIN_DRAW_SECONDS, MIN_ROOM_PLAYERS, MIN_ROUNDS, MIN_WORD_CHOICES,
    },
    model::{
        CanvasAction, CanvasSnapshot, ChatEvent, DrawEvent, DrawOp, DrawingRatingView,
        DrawingVoteUpdate, Hello, HintView, PhaseView, PlayerProfile, PlayerView, ProtocolError,
        Resume, RoomSettings, RoomSnapshot, ScoreView, Stroke, TurnResultView, Welcome,
        WordOptions,
    },
    server::ServerMessage,
};

/// Checks whether a value obeys protocol-level semantic constraints.
pub trait Validate {
    /// Returns the first detected constraint violation.
    fn validate(&self) -> Result<(), ValidationError>;
}

fn validate_count(
    field: &'static str,
    actual: usize,
    minimum: usize,
    maximum: usize,
) -> Result<(), ValidationError> {
    if actual < minimum {
        return Err(ValidationError::TooShort {
            field,
            minimum,
            actual,
        });
    }
    if actual > maximum {
        return Err(ValidationError::TooLong {
            field,
            maximum,
            actual,
        });
    }
    Ok(())
}

fn validate_range(
    field: &'static str,
    actual: u64,
    minimum: u64,
    maximum: u64,
) -> Result<(), ValidationError> {
    if !(minimum..=maximum).contains(&actual) {
        return Err(ValidationError::OutOfRange {
            field,
            minimum,
            maximum,
            actual,
        });
    }
    Ok(())
}

fn validate_plain_text(
    field: &'static str,
    value: &str,
    maximum_bytes: usize,
) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::Empty { field });
    }
    if value.len() > maximum_bytes {
        return Err(ValidationError::TooLong {
            field,
            maximum: maximum_bytes,
            actual: value.len(),
        });
    }
    if value.chars().any(char::is_control) {
        return Err(ValidationError::ControlCharacter { field });
    }
    Ok(())
}

fn validate_word(field: &'static str, word: &str) -> Result<(), ValidationError> {
    validate_plain_text(field, word, MAX_WORD_BYTES)
}

impl Validate for PlayerProfile {
    fn validate(&self) -> Result<(), ValidationError> {
        let name = self.display_name.trim();
        if name.is_empty() {
            return Err(ValidationError::Empty {
                field: "display_name",
            });
        }

        if name != self.display_name {
            return Err(ValidationError::Inconsistent {
                field: "display_name",
                reason: "leading and trailing whitespace is not allowed",
            });
        }

        if self.display_name.chars().any(char::is_control) {
            return Err(ValidationError::ControlCharacter {
                field: "display_name",
            });
        }

        let graphemes = self.display_name.graphemes(true).count();
        validate_count("display_name", graphemes, 1, MAX_NAME_GRAPHEMES)
    }
}

impl Validate for RoomSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_range(
            "rounds",
            u64::from(self.rounds),
            u64::from(MIN_ROUNDS),
            u64::from(MAX_ROUNDS),
        )?;
        validate_range(
            "draw_seconds",
            u64::from(self.draw_seconds),
            u64::from(MIN_DRAW_SECONDS),
            u64::from(MAX_DRAW_SECONDS),
        )?;
        validate_range(
            "word_choices",
            u64::from(self.word_choices),
            u64::from(MIN_WORD_CHOICES),
            u64::from(MAX_WORD_CHOICES),
        )?;
        validate_range(
            "max_players",
            u64::from(self.max_players),
            u64::from(MIN_ROOM_PLAYERS),
            MAX_PLAYERS_IN_ROOM as u64,
        )
    }
}

impl Validate for crate::model::Point {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_range("point.x", u64::from(self.x), 0, u64::from(CANVAS_WIDTH))?;
        validate_range("point.y", u64::from(self.y), 0, u64::from(CANVAS_HEIGHT))
    }
}

impl Validate for DrawOp {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Begin {
                color,
                width,
                start,
                ..
            } => {
                validate_range("draw.color", u64::from(*color), 0, 0x00ff_ffff)?;
                validate_range(
                    "draw.width",
                    u64::from(*width),
                    1,
                    u64::from(MAX_BRUSH_WIDTH),
                )?;
                start.validate()
            }
            Self::Points { points, .. } => {
                validate_count("draw.points", points.len(), 1, MAX_POINTS_PER_BATCH)?;
                points.iter().try_for_each(Validate::validate)
            }
            Self::Fill { color, at } => {
                validate_range("draw.color", u64::from(*color), 0, 0x00ff_ffff)?;
                at.validate()
            }
            Self::End { .. } | Self::Undo | Self::Clear => Ok(()),
        }
    }
}

impl Validate for Stroke {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_range("stroke.color", u64::from(self.color), 0, 0x00ff_ffff)?;
        validate_range(
            "stroke.width",
            u64::from(self.width),
            1,
            u64::from(MAX_BRUSH_WIDTH),
        )?;
        validate_count("stroke.points", self.points.len(), 1, MAX_POINTS_PER_STROKE)?;
        self.points.iter().try_for_each(Validate::validate)
    }
}

impl Validate for CanvasSnapshot {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_count("canvas.actions", self.actions.len(), 0, MAX_CANVAS_ACTIONS)?;

        let mut stroke_ids = HashSet::with_capacity(self.actions.len());
        for action in &self.actions {
            match action {
                CanvasAction::Stroke(stroke) => {
                    stroke.validate()?;
                    if !stroke_ids.insert(stroke.stroke_id) {
                        return Err(ValidationError::Inconsistent {
                            field: "canvas.actions",
                            reason: "stroke identifiers must be unique",
                        });
                    }
                }
                CanvasAction::Fill { color, at } => {
                    validate_range("canvas.fill.color", u64::from(*color), 0, 0x00ff_ffff)?;
                    at.validate()?;
                }
            }
        }
        Ok(())
    }
}

impl Validate for Hello {
    fn validate(&self) -> Result<(), ValidationError> {
        self.profile.validate()
    }
}

impl Validate for PlayerView {
    fn validate(&self) -> Result<(), ValidationError> {
        self.profile.validate()
    }
}

impl Validate for PhaseView {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_range(
            "phase.total_rounds",
            u64::from(self.total_rounds),
            u64::from(MIN_ROUNDS),
            u64::from(MAX_ROUNDS),
        )?;
        validate_range(
            "phase.round",
            u64::from(self.round),
            0,
            u64::from(self.total_rounds),
        )?;
        if let Some(masked_word) = &self.masked_word {
            validate_word("phase.masked_word", masked_word)?;
        }
        Ok(())
    }
}

impl Validate for WordOptions {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_count(
            "word_options.words",
            self.words.len(),
            usize::from(MIN_WORD_CHOICES),
            usize::from(MAX_WORD_CHOICES),
        )?;
        self.words
            .iter()
            .try_for_each(|word| validate_word("word_options.word", word))
    }
}

impl Validate for HintView {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_word("hint.masked_word", &self.masked_word)
    }
}

impl Validate for DrawEvent {
    fn validate(&self) -> Result<(), ValidationError> {
        self.operation.validate()
    }
}

impl Validate for ChatEvent {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_plain_text("chat.text", &self.text, MAX_TEXT_BYTES)
    }
}

impl Validate for ScoreView {
    fn validate(&self) -> Result<(), ValidationError> {
        let reconstructed_previous = i64::from(self.total_score) - i64::from(self.delta);
        if reconstructed_previous < 0 {
            return Err(ValidationError::Inconsistent {
                field: "score.delta",
                reason: "delta implies a negative previous score",
            });
        }
        Ok(())
    }
}

impl Validate for TurnResultView {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_word("turn_result.word", &self.word)?;
        validate_count(
            "turn_result.awards",
            self.awards.len(),
            0,
            MAX_PLAYERS_IN_ROOM,
        )?;
        let mut player_ids = HashSet::with_capacity(self.awards.len());
        for award in &self.awards {
            if award.points == 0 {
                return Err(ValidationError::OutOfRange {
                    field: "turn_result.award.points",
                    minimum: 1,
                    maximum: u64::from(u32::MAX),
                    actual: 0,
                });
            }
            if !player_ids.insert(award.player_id) {
                return Err(ValidationError::Inconsistent {
                    field: "turn_result.awards",
                    reason: "a player may have at most one award per turn",
                });
            }
        }
        Ok(())
    }
}

impl Validate for DrawingRatingView {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_rating_counts(self.likes, self.dislikes)
    }
}

impl Validate for DrawingVoteUpdate {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_rating_counts(self.likes, self.dislikes)
    }
}

fn validate_rating_counts(likes: u8, dislikes: u8) -> Result<(), ValidationError> {
    let total = usize::from(likes).saturating_add(usize::from(dislikes));
    validate_count("drawing_rating.votes", total, 0, MAX_PLAYERS_IN_ROOM)
}

impl Validate for ProtocolError {
    fn validate(&self) -> Result<(), ValidationError> {
        validate_plain_text("error.message", &self.message, MAX_TEXT_BYTES)
    }
}

impl Validate for RoomSnapshot {
    fn validate(&self) -> Result<(), ValidationError> {
        self.settings.validate()?;
        self.phase.validate()?;
        self.canvas.validate()?;

        if self.phase.total_rounds != self.settings.rounds {
            return Err(ValidationError::Inconsistent {
                field: "snapshot.phase.total_rounds",
                reason: "must match room settings",
            });
        }

        let player_limit = usize::from(self.settings.max_players);
        validate_count("snapshot.players", self.players.len(), 0, player_limit)?;
        validate_count(
            "snapshot.chat_history",
            self.chat_history.len(),
            0,
            MAX_CHAT_HISTORY,
        )?;

        let mut player_ids = HashSet::with_capacity(self.players.len());
        let mut host_count = 0usize;
        for player in &self.players {
            player.validate()?;
            if !player_ids.insert(player.player_id) {
                return Err(ValidationError::Inconsistent {
                    field: "snapshot.players",
                    reason: "player identifiers must be unique",
                });
            }
            host_count += usize::from(player.is_host);
        }
        if host_count > 1 {
            return Err(ValidationError::Inconsistent {
                field: "snapshot.players",
                reason: "at most one player may be the host",
            });
        }

        if let Some(options) = &self.word_options {
            options.validate()?;
            if options.words.len() != usize::from(self.settings.word_choices) {
                return Err(ValidationError::Inconsistent {
                    field: "snapshot.word_options",
                    reason: "option count must match room settings",
                });
            }
        }
        if let Some(secret_word) = &self.secret_word {
            validate_word("snapshot.secret_word", secret_word)?;
        }
        if let Some(result) = &self.turn_result {
            result.validate()?;
            if self.phase.phase != crate::model::GamePhase::RoundEnd
                || self.phase.turn_id != Some(result.turn_id)
            {
                return Err(ValidationError::Inconsistent {
                    field: "snapshot.turn_result",
                    reason: "must describe the current round-end turn",
                });
            }
            if result
                .awards
                .iter()
                .any(|award| !player_ids.contains(&award.player_id))
            {
                return Err(ValidationError::Inconsistent {
                    field: "snapshot.turn_result.awards",
                    reason: "award recipients must exist in the room roster",
                });
            }
        }
        if let Some(rating) = &self.drawing_rating {
            rating.validate()?;
            if self.turn_result.as_ref().map(|result| result.turn_id) != Some(rating.turn_id) {
                return Err(ValidationError::Inconsistent {
                    field: "snapshot.drawing_rating",
                    reason: "must describe the included completed turn",
                });
            }
        }
        self.chat_history.iter().try_for_each(Validate::validate)
    }
}

impl Validate for Welcome {
    fn validate(&self) -> Result<(), ValidationError> {
        self.snapshot.validate()?;
        if !self
            .snapshot
            .players
            .iter()
            .any(|player| player.player_id == self.player_id)
        {
            return Err(ValidationError::Inconsistent {
                field: "welcome.player_id",
                reason: "assigned player must exist in the snapshot",
            });
        }
        Ok(())
    }
}

impl Validate for Resume {
    fn validate(&self) -> Result<(), ValidationError> {
        self.snapshot.validate()?;
        if !self
            .snapshot
            .players
            .iter()
            .any(|player| player.player_id == self.player_id)
        {
            return Err(ValidationError::Inconsistent {
                field: "resume.player_id",
                reason: "resumed player must exist in the snapshot",
            });
        }
        Ok(())
    }
}

impl Validate for ClientMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Hello(hello) => hello.validate(),
            Self::PickWord { choice } => validate_range(
                "pick_word.choice",
                u64::from(*choice),
                0,
                u64::from(MAX_WORD_CHOICES - 1),
            ),
            Self::Draw(operation) => operation.validate(),
            Self::Guess { text } => validate_plain_text("guess.text", text, MAX_TEXT_BYTES),
            Self::Chat { text } => validate_plain_text("chat.text", text, MAX_TEXT_BYTES),
            Self::StartGame
            | Self::KickPlayer { .. }
            | Self::Rematch
            | Self::Pong { .. }
            | Self::VoteDrawing { .. } => Ok(()),
        }
    }
}

impl Validate for ServerMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Welcome(message) => message.validate(),
            Self::Resume(message) => message.validate(),
            Self::PlayerJoined(player) => player.validate(),
            Self::PhaseChanged(phase) => phase.validate(),
            Self::WordOptions(options) => options.validate(),
            Self::SecretWord { word, .. } => validate_word("secret_word.word", word),
            Self::HintRevealed(hint) => hint.validate(),
            Self::Draw(event) => event.validate(),
            Self::Chat(event) => event.validate(),
            Self::ScoreChanged(score) => score.validate(),
            Self::Error(error) => error.validate(),
            Self::TurnResult(result) => result.validate(),
            Self::DrawingVoteChanged(update) => update.validate(),
            Self::PlayerLeft { .. }
            | Self::HostChanged { .. }
            | Self::GuessResult(_)
            | Self::Ping { .. }
            | Self::Bye { .. } => Ok(()),
        }
    }
}
