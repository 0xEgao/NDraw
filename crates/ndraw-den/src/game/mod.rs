//! Synchronous server-authoritative game state machine.

mod command;
mod event;
pub mod scoring;
mod state;

use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use ndraw_proto::{
    AwardReason, CanvasSnapshot, ChatEvent, ChatKind, ClientMessage, DrawEvent, DrawingRatingView,
    DrawingVote, DrawingVoteUpdate, GuessOutcome, GuessResult, HintView, PhaseView, PlayerId,
    PlayerProfile, PlayerView, Resume, RoomSettings, RoomSnapshot, ScoreView, ServerMessage,
    TurnAward, TurnId, TurnResultView, Validate, Welcome, WordOptions, limit::MAX_CHAT_HISTORY,
};

use crate::{
    canvas::CanvasState,
    error::{JoinError, RuleError},
    guess::{contains_secret, is_close, is_correct},
    time::{GameDeadline, GameTime},
    words::WordDeck,
};

pub use command::PlayerAction;
pub use event::{Audience, EmittedEvent};
pub use state::{GameStateView, PlayerState};

use self::{
    scoring::guess_score,
    state::{ChoosingWordState, DrawingState, LobbyState, PhaseState, RoundEndState},
};

/// Duration given to the drawer to select a word.
pub const WORD_CHOICE_DURATION: Duration = Duration::from_secs(15);

/// Duration for which turn results remain visible.
pub const ROUND_END_DURATION: Duration = Duration::from_secs(6);

const HINT_REMAINING_SECONDS: [u64; 3] = [60, 30, 10];

/// Deterministic authoritative game state for one room.
#[derive(Debug)]
pub struct Game {
    settings: RoomSettings,
    players: HashMap<PlayerId, PlayerState>,
    order: Vec<PlayerId>,
    host: Option<PlayerId>,
    phase: PhaseState,
    round: u8,
    remaining_drawers: VecDeque<PlayerId>,
    next_player_id: u32,
    next_turn_id: u32,
    next_joined_order: u64,
    words: WordDeck,
    unix_origin_ms: u64,
    chat_history: VecDeque<ChatEvent>,
}

impl Game {
    /// Constructs a validated lobby.
    ///
    /// `unix_origin_ms` maps deterministic room-relative deadlines into the
    /// timestamps displayed by clients. Tests can pass any fixed value.
    pub fn new(
        settings: RoomSettings,
        words: Vec<String>,
        random_seed: [u8; 32],
        unix_origin_ms: u64,
        lobby_deadline: GameDeadline,
    ) -> Result<Self, crate::error::GameBuildError> {
        settings.validate()?;
        let word_choices = usize::from(settings.word_choices);
        let words = WordDeck::new(words, word_choices, random_seed)?;
        Ok(Self {
            settings,
            players: HashMap::new(),
            order: Vec::new(),
            host: None,
            phase: PhaseState::Lobby(LobbyState {
                deadline: lobby_deadline,
            }),
            round: 0,
            remaining_drawers: VecDeque::new(),
            next_player_id: 1,
            next_turn_id: 1,
            next_joined_order: 0,
            words,
            unix_origin_ms,
            chat_history: VecDeque::new(),
        })
    }

    /// Returns immutable room configuration.
    #[must_use]
    pub const fn settings(&self) -> RoomSettings {
        self.settings
    }

    /// Returns whether new identities may enter the room.
    #[must_use]
    pub fn is_lobby(&self) -> bool {
        matches!(self.phase, PhaseState::Lobby(_))
    }

    /// Returns whether this player identity is retained by the game.
    #[must_use]
    pub fn has_player(&self, player_id: PlayerId) -> bool {
        self.players.contains_key(&player_id)
    }

    /// Returns whether this player currently has a connection.
    #[must_use]
    pub fn is_connected(&self, player_id: PlayerId) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.connected)
    }

    /// Number of players with an attached connection.
    #[must_use]
    pub fn connected_count(&self) -> usize {
        self.players
            .values()
            .filter(|player| player.connected)
            .count()
    }

    /// Adds a new identity to the initial lobby.
    pub fn add_player(
        &mut self,
        profile: PlayerProfile,
        claim_host: bool,
    ) -> Result<(PlayerId, Vec<EmittedEvent>), JoinError> {
        profile.validate().map_err(JoinError::InvalidProfile)?;
        if !self.is_lobby() {
            return Err(JoinError::GameAlreadyStarted);
        }
        if self.players.len() >= usize::from(self.settings.max_players) {
            return Err(JoinError::RoomFull);
        }

        let player_id = PlayerId(self.next_player_id);
        self.next_player_id = self
            .next_player_id
            .checked_add(1)
            .ok_or(JoinError::PlayerIdExhausted)?;
        let joined_order = self.next_joined_order;
        self.next_joined_order = self.next_joined_order.saturating_add(1);
        self.players.insert(
            player_id,
            PlayerState {
                player_id,
                profile,
                score: 0,
                connected: true,
                joined_order,
            },
        );
        self.order.push(player_id);

        let mut events = Vec::new();
        if claim_host && self.host.is_none() {
            self.host = Some(player_id);
            events.push(EmittedEvent::everyone_except(
                player_id,
                ServerMessage::HostChanged { player_id },
            ));
        }
        if let Some(view) = self.player_view(player_id) {
            events.push(EmittedEvent::everyone_except(
                player_id,
                ServerMessage::PlayerJoined(view),
            ));
        }
        Ok((player_id, events))
    }

    /// Marks a retained player as connected and returns presence events.
    pub fn reconnect_player(
        &mut self,
        player_id: PlayerId,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        let was_connected = self
            .players
            .get(&player_id)
            .ok_or(RuleError::UnknownPlayer)?
            .connected;
        if was_connected {
            return Ok(Vec::new());
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            player.connected = true;
        }

        let mut events = Vec::new();
        if self.host.is_none() {
            self.host = Some(player_id);
            events.push(EmittedEvent::everyone_except(
                player_id,
                ServerMessage::HostChanged { player_id },
            ));
        }
        if let Some(view) = self.player_view(player_id) {
            events.push(EmittedEvent::everyone_except(
                player_id,
                ServerMessage::PlayerJoined(view),
            ));
        }
        Ok(events)
    }

    /// Marks one connection generation as departed without deleting score or rotation.
    pub fn disconnect_player(
        &mut self,
        player_id: PlayerId,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(RuleError::UnknownPlayer)?;
        if !player.connected {
            return Ok(Vec::new());
        }
        player.connected = false;

        if let PhaseState::Drawing(drawing) = &mut self.phase {
            if drawing.drawer == player_id {
                drawing.canvas.finalize_active();
            }
        }

        let mut events = vec![EmittedEvent::everyone(ServerMessage::PlayerLeft {
            player_id,
        })];
        if self.host == Some(player_id) {
            self.host = self.oldest_connected_player();
            if let Some(new_host) = self.host {
                events.push(EmittedEvent::everyone(ServerMessage::HostChanged {
                    player_id: new_host,
                }));
            }
        }

        let was_choosing_player = matches!(
            &self.phase,
            PhaseState::ChoosingWord(state) if state.drawer == player_id
        );
        if was_choosing_player {
            events.extend(self.begin_next_turn(now)?);
            return Ok(events);
        }

        if self.all_active_guessers_correct() {
            events.extend(self.finish_turn(now)?);
        }
        Ok(events)
    }

    /// Applies one authenticated action at a caller-supplied monotonic time.
    pub fn apply(
        &mut self,
        player_id: PlayerId,
        action: PlayerAction,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        let player = self
            .players
            .get(&player_id)
            .ok_or(RuleError::UnknownPlayer)?;
        if !player.connected {
            return Err(RuleError::PlayerDisconnected);
        }
        if self
            .next_deadline()
            .is_some_and(|deadline| deadline.is_due(now))
        {
            return Err(RuleError::DeadlineElapsed);
        }

        match action {
            PlayerAction::StartGame => self.start_game(player_id, now),
            PlayerAction::PickWord { choice } => self.pick_word(player_id, choice, now),
            PlayerAction::Draw(operation) => self.draw(player_id, operation),
            PlayerAction::Guess { text } => self.guess(player_id, text, now),
            PlayerAction::Chat { text } => self.chat(player_id, text),
            PlayerAction::KickPlayer { player_id: target } => {
                self.kick_player(player_id, target, now)
            }
            PlayerAction::Rematch => self.rematch(player_id, now),
            PlayerAction::VoteDrawing { turn_id, vote } => {
                self.vote_drawing(player_id, turn_id, vote)
            }
        }
    }

    /// Advances one due hint or phase deadline.
    pub fn handle_deadline(&mut self, now: GameTime) -> Result<Vec<EmittedEvent>, RuleError> {
        let Some(deadline) = self.next_deadline() else {
            return Ok(Vec::new());
        };
        if !deadline.is_due(now) {
            return Ok(Vec::new());
        }

        let phase_action = match &self.phase {
            PhaseState::ChoosingWord(state) => (0_u8, Some(state.drawer)),
            PhaseState::Drawing(state) if state.deadline.is_due(now) => (1, None),
            PhaseState::Drawing(_) => (2, None),
            PhaseState::RoundEnd(_) => (3, None),
            PhaseState::Lobby(_) | PhaseState::GameOver => (4, None),
        };
        match phase_action {
            (0, Some(drawer)) => self.pick_word(drawer, 0, now),
            (1, _) => self.finish_turn(now),
            (2, _) => self.reveal_hint(),
            (3, _) => self.begin_next_turn(now),
            _ => Ok(Vec::new()),
        }
    }

    /// Returns the next gameplay deadline, excluding room-lifecycle expiry.
    #[must_use]
    pub fn next_deadline(&self) -> Option<GameDeadline> {
        match &self.phase {
            PhaseState::ChoosingWord(state) => Some(state.deadline),
            PhaseState::Drawing(state) => state
                .hint_deadlines
                .get(state.next_hint)
                .copied()
                .map_or(Some(state.deadline), |hint| Some(hint.min(state.deadline))),
            PhaseState::RoundEnd(state) => Some(state.deadline),
            PhaseState::Lobby(_) | PhaseState::GameOver => None,
        }
    }

    /// Returns a private room snapshot safe for the specified viewer.
    #[must_use]
    pub fn snapshot_for(&self, viewer: PlayerId) -> RoomSnapshot {
        let (canvas, word_options, secret_word, turn_result, drawing_rating) = match &self.phase {
            PhaseState::ChoosingWord(state) if state.drawer == viewer => (
                CanvasSnapshot::default(),
                Some(WordOptions {
                    turn_id: state.turn_id,
                    words: state.options.clone(),
                }),
                None,
                None,
                None,
            ),
            PhaseState::Drawing(state) => (
                state.canvas.snapshot(),
                None,
                (state.drawer == viewer).then(|| state.secret_word.clone()),
                None,
                None,
            ),
            PhaseState::RoundEnd(state) => (
                state.canvas.snapshot(),
                None,
                None,
                Some(TurnResultView {
                    turn_id: state.turn_id,
                    word: state.secret_word.clone(),
                    awards: state.awards.clone(),
                }),
                Some(rating_view(state, viewer)),
            ),
            _ => (CanvasSnapshot::default(), None, None, None, None),
        };

        RoomSnapshot {
            settings: self.settings,
            players: self
                .order
                .iter()
                .filter_map(|player_id| self.player_view(*player_id))
                .collect(),
            phase: self.phase_view(),
            canvas,
            word_options,
            secret_word,
            chat_history: self.chat_history.iter().cloned().collect(),
            turn_result,
            drawing_rating,
        }
    }

    /// Builds a new-session message for a viewer.
    #[must_use]
    pub fn welcome_for(&self, player_id: PlayerId, room_code: ndraw_proto::RoomCode) -> Welcome {
        Welcome {
            player_id,
            room_code,
            snapshot: self.snapshot_for(player_id),
        }
    }

    /// Builds a resumed-session message for a viewer.
    #[must_use]
    pub fn resume_for(&self, player_id: PlayerId) -> Resume {
        Resume {
            player_id,
            snapshot: self.snapshot_for(player_id),
        }
    }

    /// Returns a compact immutable diagnostic view.
    #[must_use]
    pub fn state_view(&self) -> GameStateView {
        let (drawer, turn_id) = self.current_turn();
        GameStateView {
            phase: self.phase.public_phase(),
            round: self.round,
            host: self.host,
            drawer,
            turn_id,
            connected_players: self.connected_count(),
            scores: self
                .order
                .iter()
                .filter_map(|player_id| {
                    self.players
                        .get(player_id)
                        .map(|player| (*player_id, player.score))
                })
                .collect(),
        }
    }

    fn start_game(
        &mut self,
        player_id: PlayerId,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        self.require_host(player_id)?;
        if !matches!(self.phase, PhaseState::Lobby(_)) {
            return Err(RuleError::InvalidPhase);
        }
        if self.connected_count() < 2 {
            return Err(RuleError::NotEnoughPlayers);
        }

        self.round = 1;
        self.refill_drawers();
        self.begin_next_turn(now)
    }

    fn pick_word(
        &mut self,
        player_id: PlayerId,
        choice: u8,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        let (drawer, turn_id, selected) = match &self.phase {
            PhaseState::ChoosingWord(state) => {
                if state.drawer != player_id {
                    return Err(RuleError::DrawerOnly);
                }
                let selected = state
                    .options
                    .get(usize::from(choice))
                    .cloned()
                    .ok_or(RuleError::InvalidWordChoice)?;
                (state.drawer, state.turn_id, selected)
            }
            _ => return Err(RuleError::InvalidPhase),
        };

        let duration = Duration::from_secs(u64::from(self.settings.draw_seconds));
        let deadline = GameDeadline::after(now, duration);
        let word_characters: Vec<char> = selected.chars().collect();
        let revealed = word_characters
            .iter()
            .map(|character| !character.is_alphanumeric())
            .collect();
        let hint_deadlines = HINT_REMAINING_SECONDS
            .into_iter()
            .filter(|remaining| *remaining < u64::from(self.settings.draw_seconds))
            .map(|remaining| {
                GameDeadline::after(
                    now,
                    Duration::from_secs(u64::from(self.settings.draw_seconds) - remaining),
                )
            })
            .collect();

        self.phase = PhaseState::Drawing(DrawingState {
            drawer,
            turn_id,
            secret_word: selected.clone(),
            word_characters,
            revealed,
            guessed: Default::default(),
            guess_awards: Default::default(),
            guesser_score_total: 0,
            canvas: CanvasState::default(),
            deadline,
            hint_deadlines,
            next_hint: 0,
        });

        Ok(vec![
            EmittedEvent::player(
                drawer,
                ServerMessage::SecretWord {
                    turn_id,
                    word: selected,
                },
            ),
            EmittedEvent::everyone(ServerMessage::PhaseChanged(self.phase_view())),
        ])
    }

    fn draw(
        &mut self,
        player_id: PlayerId,
        operation: ndraw_proto::DrawOp,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        let state = match &mut self.phase {
            PhaseState::Drawing(state) => state,
            _ => return Err(RuleError::InvalidPhase),
        };
        if state.drawer != player_id {
            return Err(RuleError::DrawerOnly);
        }
        state.canvas.apply(&operation)?;
        Ok(vec![EmittedEvent::everyone(ServerMessage::Draw(
            DrawEvent {
                player_id,
                turn_id: state.turn_id,
                operation,
            },
        ))])
    }

    fn guess(
        &mut self,
        player_id: PlayerId,
        text: String,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        ClientMessage::Guess { text: text.clone() }
            .validate()
            .map_err(RuleError::InvalidMessage)?;

        let (drawer, secret_word, deadline, already_guessed) = match &self.phase {
            PhaseState::Drawing(state) => (
                state.drawer,
                state.secret_word.clone(),
                state.deadline,
                state.guessed.contains(&player_id),
            ),
            _ => return Err(RuleError::InvalidPhase),
        };
        if drawer == player_id {
            return Err(RuleError::DrawerCannotGuess);
        }
        if already_guessed {
            return Err(RuleError::AlreadyGuessed);
        }

        if is_correct(&text, &secret_word) {
            let duration_ms = u64::from(self.settings.draw_seconds).saturating_mul(1_000);
            let points = guess_score(deadline, now, duration_ms);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.score = player.score.saturating_add(points);
            }
            if let PhaseState::Drawing(state) = &mut self.phase {
                state.guessed.insert(player_id);
                state.guess_awards.insert(player_id, points);
                state.guesser_score_total = state.guesser_score_total.saturating_add(points);
            }

            let total_score = self
                .players
                .get(&player_id)
                .map_or(points, |player| player.score);
            let delta = positive_score_delta(points);
            let mut events = vec![
                EmittedEvent::everyone(ServerMessage::GuessResult(GuessResult {
                    player_id,
                    outcome: GuessOutcome::Correct { points },
                })),
                EmittedEvent::everyone(ServerMessage::ScoreChanged(ScoreView {
                    player_id,
                    total_score,
                    delta,
                })),
            ];
            if self.all_active_guessers_correct() {
                events.extend(self.finish_turn(now)?);
            }
            return Ok(events);
        }

        let outcome = if is_close(&text, &secret_word) {
            GuessOutcome::Close
        } else {
            GuessOutcome::Incorrect
        };
        let mut events = Vec::with_capacity(2);
        // A non-exact sentence can still contain the complete secret. Preserve
        // the original private guess outcome without broadcasting that text.
        if !contains_secret(&text, &secret_word) {
            let event = self.retain_chat(player_id, ChatKind::Guess, text);
            events.push(EmittedEvent::everyone(ServerMessage::Chat(event)));
        }
        events.push(EmittedEvent::player(
            player_id,
            ServerMessage::GuessResult(GuessResult { player_id, outcome }),
        ));
        Ok(events)
    }

    fn chat(&mut self, player_id: PlayerId, text: String) -> Result<Vec<EmittedEvent>, RuleError> {
        ClientMessage::Chat { text: text.clone() }
            .validate()
            .map_err(RuleError::InvalidMessage)?;
        if let PhaseState::Drawing(state) = &self.phase {
            if contains_secret(&text, &state.secret_word) {
                return Err(RuleError::Spoiler);
            }
        }

        let event = self.retain_chat(player_id, ChatKind::Chat, text);
        Ok(vec![EmittedEvent::everyone(ServerMessage::Chat(event))])
    }

    fn retain_chat(&mut self, player_id: PlayerId, kind: ChatKind, text: String) -> ChatEvent {
        let event = ChatEvent {
            player_id,
            kind,
            text,
        };
        self.chat_history.push_back(event.clone());
        while self.chat_history.len() > MAX_CHAT_HISTORY {
            self.chat_history.pop_front();
        }
        event
    }

    fn vote_drawing(
        &mut self,
        player_id: PlayerId,
        turn_id: TurnId,
        vote: Option<DrawingVote>,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        let state = match &mut self.phase {
            PhaseState::RoundEnd(state) => state,
            _ => return Err(RuleError::InvalidPhase),
        };
        if state.turn_id != turn_id {
            return Err(RuleError::WrongTurn);
        }
        if state.drawer == player_id {
            return Err(RuleError::DrawerCannotVote);
        }

        match vote {
            Some(value) => {
                state.votes.insert(player_id, value);
            }
            None => {
                state.votes.remove(&player_id);
            }
        }
        let (likes, dislikes) = rating_counts(&state.votes);
        Ok(vec![EmittedEvent::everyone(
            ServerMessage::DrawingVoteChanged(DrawingVoteUpdate {
                turn_id,
                player_id,
                vote,
                likes,
                dislikes,
            }),
        )])
    }

    fn kick_player(
        &mut self,
        actor: PlayerId,
        target: PlayerId,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        self.require_host(actor)?;
        if actor == target {
            return Err(RuleError::CannotKickSelf);
        }
        if !self.players.contains_key(&target) {
            return Err(RuleError::InvalidKickTarget);
        }
        if matches!(&self.phase, PhaseState::Drawing(state) if state.drawer == target) {
            return Err(RuleError::CannotKickDrawer);
        }

        self.players.remove(&target);
        self.order.retain(|player_id| *player_id != target);
        self.remaining_drawers
            .retain(|player_id| *player_id != target);
        if let PhaseState::Drawing(state) = &mut self.phase {
            state.guessed.remove(&target);
        }
        let mut events = vec![EmittedEvent::everyone(ServerMessage::PlayerLeft {
            player_id: target,
        })];
        if self.all_active_guessers_correct() {
            events.extend(self.finish_turn(now)?);
        }
        Ok(events)
    }

    fn rematch(
        &mut self,
        player_id: PlayerId,
        now: GameTime,
    ) -> Result<Vec<EmittedEvent>, RuleError> {
        self.require_host(player_id)?;
        if !matches!(self.phase, PhaseState::GameOver) {
            return Err(RuleError::InvalidPhase);
        }
        if self.connected_count() < 2 {
            return Err(RuleError::NotEnoughPlayers);
        }

        let mut events = Vec::new();
        for ordered_player_id in &self.order {
            if let Some(player) = self.players.get_mut(ordered_player_id) {
                let previous = player.score;
                player.score = 0;
                if previous > 0 {
                    let delta = match i32::try_from(previous) {
                        Ok(value) => value.saturating_neg(),
                        Err(_) => i32::MIN + 1,
                    };
                    events.push(EmittedEvent::everyone(ServerMessage::ScoreChanged(
                        ScoreView {
                            player_id: *ordered_player_id,
                            total_score: 0,
                            delta,
                        },
                    )));
                }
            }
        }
        self.round = 1;
        self.remaining_drawers.clear();
        self.refill_drawers();
        events.extend(self.begin_next_turn(now)?);
        Ok(events)
    }

    fn begin_next_turn(&mut self, now: GameTime) -> Result<Vec<EmittedEvent>, RuleError> {
        loop {
            while let Some(candidate) = self.remaining_drawers.pop_front() {
                if self.is_connected(candidate) {
                    let turn_id = TurnId(self.next_turn_id);
                    self.next_turn_id = self
                        .next_turn_id
                        .checked_add(1)
                        .ok_or(RuleError::TurnIdExhausted)?;
                    let options = self.words.offer(usize::from(self.settings.word_choices));
                    self.phase = PhaseState::ChoosingWord(ChoosingWordState {
                        drawer: candidate,
                        turn_id,
                        options: options.clone(),
                        deadline: GameDeadline::after(now, WORD_CHOICE_DURATION),
                    });
                    return Ok(vec![
                        EmittedEvent::everyone(ServerMessage::PhaseChanged(self.phase_view())),
                        EmittedEvent::player(
                            candidate,
                            ServerMessage::WordOptions(WordOptions {
                                turn_id,
                                words: options,
                            }),
                        ),
                    ]);
                }
            }

            if self.round >= self.settings.rounds || self.connected_count() == 0 {
                self.phase = PhaseState::GameOver;
                return Ok(vec![EmittedEvent::everyone(ServerMessage::PhaseChanged(
                    self.phase_view(),
                ))]);
            }
            self.round = self.round.saturating_add(1);
            self.refill_drawers();
        }
    }

    fn finish_turn(&mut self, now: GameTime) -> Result<Vec<EmittedEvent>, RuleError> {
        let old_phase = std::mem::replace(&mut self.phase, PhaseState::GameOver);
        let mut drawing = match old_phase {
            PhaseState::Drawing(drawing) => drawing,
            phase => {
                self.phase = phase;
                return Err(RuleError::InvalidPhase);
            }
        };
        drawing.canvas.finalize_active();

        let drawer_bonus = drawing.guesser_score_total / 4;
        let mut events = Vec::new();
        if drawer_bonus > 0 {
            if let Some(drawer) = self.players.get_mut(&drawing.drawer) {
                drawer.score = drawer.score.saturating_add(drawer_bonus);
                let delta = positive_score_delta(drawer_bonus);
                events.push(EmittedEvent::everyone(ServerMessage::ScoreChanged(
                    ScoreView {
                        player_id: drawing.drawer,
                        total_score: drawer.score,
                        delta,
                    },
                )));
            }
        }

        let mut awards = Vec::new();
        for player_id in &self.order {
            if let Some(points) = drawing.guess_awards.get(player_id).copied() {
                awards.push(TurnAward {
                    player_id: *player_id,
                    points,
                    reason: AwardReason::CorrectGuess,
                });
            } else if *player_id == drawing.drawer && drawer_bonus > 0 {
                awards.push(TurnAward {
                    player_id: *player_id,
                    points: drawer_bonus,
                    reason: AwardReason::DrawerBonus,
                });
            }
        }

        let result = TurnResultView {
            turn_id: drawing.turn_id,
            word: drawing.secret_word.clone(),
            awards: awards.clone(),
        };

        self.phase = PhaseState::RoundEnd(RoundEndState {
            drawer: drawing.drawer,
            turn_id: drawing.turn_id,
            secret_word: drawing.secret_word,
            canvas: drawing.canvas,
            awards,
            votes: HashMap::new(),
            deadline: GameDeadline::after(now, ROUND_END_DURATION),
        });
        events.push(EmittedEvent::everyone(ServerMessage::TurnResult(result)));
        events.push(EmittedEvent::everyone(ServerMessage::PhaseChanged(
            self.phase_view(),
        )));
        Ok(events)
    }

    fn reveal_hint(&mut self) -> Result<Vec<EmittedEvent>, RuleError> {
        let (turn_id, masked_word) = match &mut self.phase {
            PhaseState::Drawing(state) => {
                if state.hint_deadlines.get(state.next_hint).is_none() {
                    return Err(RuleError::InvalidPhase);
                }
                state.next_hint = state.next_hint.saturating_add(1);
                if let Some(index) = state.revealed.iter().position(|revealed| !revealed) {
                    if let Some(value) = state.revealed.get_mut(index) {
                        *value = true;
                    }
                }
                (state.turn_id, masked_word(state))
            }
            _ => return Err(RuleError::InvalidPhase),
        };
        Ok(vec![EmittedEvent::everyone(ServerMessage::HintRevealed(
            HintView {
                turn_id,
                masked_word,
            },
        ))])
    }

    fn require_host(&self, player_id: PlayerId) -> Result<(), RuleError> {
        if self.host == Some(player_id) {
            Ok(())
        } else {
            Err(RuleError::HostOnly)
        }
    }

    fn refill_drawers(&mut self) {
        let drawers = self
            .order
            .iter()
            .copied()
            .filter(|player_id| self.is_connected(*player_id))
            .collect();
        self.remaining_drawers = drawers;
    }

    fn oldest_connected_player(&self) -> Option<PlayerId> {
        self.order
            .iter()
            .copied()
            .find(|player_id| self.is_connected(*player_id))
    }

    fn all_active_guessers_correct(&self) -> bool {
        let PhaseState::Drawing(state) = &self.phase else {
            return false;
        };
        let eligible: Vec<PlayerId> = self
            .order
            .iter()
            .copied()
            .filter(|player_id| *player_id != state.drawer && self.is_connected(*player_id))
            .collect();
        !eligible.is_empty()
            && eligible
                .iter()
                .all(|player_id| state.guessed.contains(player_id))
    }

    fn player_view(&self, player_id: PlayerId) -> Option<PlayerView> {
        let player = self.players.get(&player_id)?;
        let has_guessed = matches!(
            &self.phase,
            PhaseState::Drawing(state) if state.guessed.contains(&player_id)
        );
        Some(PlayerView {
            player_id,
            profile: player.profile.clone(),
            score: player.score,
            connected: player.connected,
            is_host: self.host == Some(player_id),
            has_guessed,
        })
    }

    fn current_turn(&self) -> (Option<PlayerId>, Option<TurnId>) {
        match &self.phase {
            PhaseState::ChoosingWord(state) => (Some(state.drawer), Some(state.turn_id)),
            PhaseState::Drawing(state) => (Some(state.drawer), Some(state.turn_id)),
            PhaseState::RoundEnd(state) => (Some(state.drawer), Some(state.turn_id)),
            PhaseState::Lobby(_) | PhaseState::GameOver => (None, None),
        }
    }

    fn phase_view(&self) -> PhaseView {
        let (drawer, turn_id) = self.current_turn();
        let (deadline, masked_word) = match &self.phase {
            PhaseState::Lobby(state) => (Some(state.deadline), None),
            PhaseState::ChoosingWord(state) => (Some(state.deadline), None),
            PhaseState::Drawing(state) => (Some(state.deadline), Some(masked_word(state))),
            PhaseState::RoundEnd(state) => (Some(state.deadline), Some(state.secret_word.clone())),
            PhaseState::GameOver => (None, None),
        };
        PhaseView {
            phase: self.phase.public_phase(),
            round: self.round,
            total_rounds: self.settings.rounds,
            drawer,
            turn_id,
            deadline_unix_ms: deadline
                .map(|deadline| self.unix_origin_ms.saturating_add(deadline.0)),
            masked_word,
        }
    }
}

fn rating_counts(votes: &HashMap<PlayerId, DrawingVote>) -> (u8, u8) {
    let likes = votes
        .values()
        .filter(|vote| **vote == DrawingVote::Like)
        .count();
    let dislikes = votes.len().saturating_sub(likes);
    (
        u8::try_from(likes).map_or(u8::MAX, |value| value),
        u8::try_from(dislikes).map_or(u8::MAX, |value| value),
    )
}

fn rating_view(state: &RoundEndState, viewer: PlayerId) -> DrawingRatingView {
    let (likes, dislikes) = rating_counts(&state.votes);
    DrawingRatingView {
        turn_id: state.turn_id,
        likes,
        dislikes,
        viewer_vote: state.votes.get(&viewer).copied(),
    }
}

fn masked_word(state: &DrawingState) -> String {
    state
        .word_characters
        .iter()
        .zip(&state.revealed)
        .map(|(character, revealed)| if *revealed { *character } else { '_' })
        .collect()
}

fn positive_score_delta(score: u32) -> i32 {
    if score > i32::MAX as u32 {
        i32::MAX
    } else {
        score as i32
    }
}
