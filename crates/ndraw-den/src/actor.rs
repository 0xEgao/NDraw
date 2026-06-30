//! Bounded single-owner room actor.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ndraw_proto::{
    ByeReason, ClientToken, PlayerId, PlayerProfile, RoomCode, RoomSettings, ServerMessage,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::Instant,
};

use crate::{
    Audience, EmittedEvent, Game, GameBuildError, GameDeadline, GameTime, JoinError, MailboxError,
    PlayerAction, RuleError, session::ConnectionLease,
};

const COMMAND_BURST: usize = 64;

/// Immutable inputs used to start one room actor.
#[derive(Debug, Clone)]
pub struct RoomConfig {
    /// Public room identity.
    pub room_code: RoomCode,
    /// Directory ownership generation used to fence stale actor exits.
    pub room_generation: u64,
    /// Token supplied while creating the room; its first join claims host.
    pub creator_token: ClientToken,
    /// Validated gameplay settings.
    pub settings: RoomSettings,
    /// Word pool used for deterministic offers.
    pub words: Vec<String>,
    /// Reproducible word-shuffle seed.
    pub random_seed: [u8; 32],
    /// Capacity of the actor's bounded command mailbox.
    pub command_capacity: usize,
    /// Maximum time an unstarted lobby remains alive.
    pub lobby_timeout: Duration,
    /// Grace period after the last connected player leaves.
    pub empty_timeout: Duration,
    /// Optional lifecycle notification consumed by a room directory.
    pub exit_tx: Option<mpsc::UnboundedSender<RoomExit>>,
}

/// Connection data supplied by the WebSocket integration layer.
#[derive(Debug)]
pub struct JoinRequest {
    /// Stable anonymous browser identity.
    pub client_token: ClientToken,
    /// Display information for a new identity.
    pub profile: PlayerProfile,
    /// Bounded per-connection queue drained by the socket writer task.
    pub outbound: mpsc::Sender<Arc<ServerMessage>>,
}

/// Successful actor attachment returned to a socket reader task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinAccepted {
    /// Lease required for later action and departure messages.
    pub lease: ConnectionLease,
    /// Whether an existing player identity was reclaimed.
    pub resumed: bool,
}

/// Why a room actor ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomExitReason {
    /// No game began before the lobby deadline.
    LobbyExpired,
    /// No players reconnected before the empty-room deadline.
    Empty,
    /// The owner requested an orderly shutdown.
    Shutdown,
    /// Every command sender was dropped.
    MailboxClosed,
}

/// Lifecycle message used by a directory to remove the exact actor generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomExit {
    /// Room whose task ended.
    pub room_code: RoomCode,
    /// Ownership generation of the task that ended.
    pub room_generation: u64,
    /// Terminal reason.
    pub reason: RoomExitReason,
}

/// Cheap cloneable command endpoint for a room actor.
#[derive(Debug, Clone)]
pub struct RoomHandle {
    room_code: RoomCode,
    room_generation: u64,
    tx: mpsc::Sender<RoomCommand>,
}

impl RoomHandle {
    /// Returns the room identity associated with this endpoint.
    #[must_use]
    pub const fn room_code(&self) -> RoomCode {
        self.room_code
    }

    /// Returns the directory ownership generation.
    #[must_use]
    pub const fn room_generation(&self) -> u64 {
        self.room_generation
    }

    /// Attaches or resumes one client, waiting only for bounded mailbox space.
    pub async fn join(&self, request: JoinRequest) -> Result<JoinAccepted, JoinError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RoomCommand::Join {
                request,
                reply: reply_tx,
            })
            .await
            .map_err(|_| JoinError::RoomClosed)?;
        reply_rx.await.map_err(|_| JoinError::RoomClosed)?
    }

    /// Enqueues an authenticated gameplay action without waiting.
    pub fn try_action(
        &self,
        lease: ConnectionLease,
        action: PlayerAction,
    ) -> Result<(), MailboxError> {
        self.try_send(RoomCommand::Action { lease, action })
    }

    /// Reports that one socket reader or writer stopped.
    pub fn try_leave(&self, lease: ConnectionLease) -> Result<(), MailboxError> {
        self.try_send(RoomCommand::Leave { lease })
    }

    /// Requests graceful actor termination.
    pub fn try_shutdown(&self) -> Result<(), MailboxError> {
        self.try_send(RoomCommand::Shutdown)
    }

    fn try_send(&self, command: RoomCommand) -> Result<(), MailboxError> {
        self.tx.try_send(command).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => MailboxError::Full,
            mpsc::error::TrySendError::Closed(_) => MailboxError::Closed,
        })
    }
}

/// Spawn result retaining both the public handle and join handle for tests and shutdown.
#[derive(Debug)]
pub struct RoomTask {
    /// Public bounded command endpoint.
    pub handle: RoomHandle,
    /// Tokio task running the actor.
    pub task: JoinHandle<RoomExitReason>,
}

/// Constructs and spawns a room actor.
pub fn spawn_room(config: RoomConfig) -> Result<RoomTask, GameBuildError> {
    let capacity = config.command_capacity.max(1);
    let (tx, rx) = mpsc::channel(capacity);
    let handle = RoomHandle {
        room_code: config.room_code,
        room_generation: config.room_generation,
        tx,
    };
    let start = Instant::now();
    let unix_origin_ms = unix_milliseconds();
    let lobby_deadline = GameDeadline::after(GameTime::default(), config.lobby_timeout);
    let game = Game::new(
        config.settings,
        config.words.clone(),
        config.random_seed,
        unix_origin_ms,
        lobby_deadline,
    )?;
    let task = tokio::spawn(RoomActor::new(config, game, rx, start).run());
    Ok(RoomTask { handle, task })
}

#[derive(Debug)]
enum RoomCommand {
    Join {
        request: JoinRequest,
        reply: oneshot::Sender<Result<JoinAccepted, JoinError>>,
    },
    Action {
        lease: ConnectionLease,
        action: PlayerAction,
    },
    Leave {
        lease: ConnectionLease,
    },
    Shutdown,
}

#[derive(Debug)]
struct Session {
    player_id: PlayerId,
    generation: u64,
    outbound: Option<mpsc::Sender<Arc<ServerMessage>>>,
}

struct RoomActor {
    config: RoomConfig,
    game: Game,
    rx: mpsc::Receiver<RoomCommand>,
    started_at: Instant,
    sessions: HashMap<ClientToken, Session>,
    token_by_player: HashMap<PlayerId, ClientToken>,
    kicked: HashSet<ClientToken>,
    ever_joined: bool,
    empty_since: Option<GameTime>,
}

impl RoomActor {
    fn new(
        config: RoomConfig,
        game: Game,
        rx: mpsc::Receiver<RoomCommand>,
        started_at: Instant,
    ) -> Self {
        Self {
            config,
            game,
            rx,
            started_at,
            sessions: HashMap::new(),
            token_by_player: HashMap::new(),
            kicked: HashSet::new(),
            ever_joined: false,
            empty_since: None,
        }
    }

    async fn run(mut self) -> RoomExitReason {
        let reason = loop {
            let now = self.now();
            if let Some(reason) = self.lifecycle_expiry(now) {
                break reason;
            }
            if self
                .game
                .next_deadline()
                .is_some_and(|deadline| deadline.is_due(now))
            {
                self.advance_deadlines(now);
                continue;
            }

            let wake_at = self.next_wake(now);
            let sleep = tokio::time::sleep_until(wake_at);
            tokio::pin!(sleep);
            tokio::select! {
                biased;
                command = self.rx.recv() => {
                    let Some(command) = command else {
                        break RoomExitReason::MailboxClosed;
                    };
                    if self.process_command(command) {
                        break RoomExitReason::Shutdown;
                    }
                    for _ in 1..COMMAND_BURST {
                        match self.rx.try_recv() {
                            Ok(command) => {
                                if self.process_command(command) {
                                    self.close_all(ByeReason::ServerShutdown);
                                    return self.finish(RoomExitReason::Shutdown);
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                () = &mut sleep => {}
            }
        };

        let bye = if reason == RoomExitReason::Shutdown {
            ByeReason::ServerShutdown
        } else {
            ByeReason::RoomClosed
        };
        self.close_all(bye);
        self.finish(reason)
    }

    fn finish(&self, reason: RoomExitReason) -> RoomExitReason {
        if let Some(exit_tx) = &self.config.exit_tx {
            let _ignored = exit_tx.send(RoomExit {
                room_code: self.config.room_code,
                room_generation: self.config.room_generation,
                reason,
            });
        }
        reason
    }

    fn process_command(&mut self, command: RoomCommand) -> bool {
        let now = self.now();
        self.advance_deadlines(now);
        match command {
            RoomCommand::Join { request, reply } => {
                let result = self.join(request, now);
                let _ignored = reply.send(result);
            }
            RoomCommand::Action { lease, action } => self.action(lease, action, now),
            RoomCommand::Leave { lease } => self.leave(lease, now),
            RoomCommand::Shutdown => return true,
        }
        self.update_empty_deadline(now);
        false
    }

    fn join(&mut self, request: JoinRequest, now: GameTime) -> Result<JoinAccepted, JoinError> {
        if self.kicked.contains(&request.client_token) {
            return Err(JoinError::Kicked);
        }

        if let Some(session) = self.sessions.get_mut(&request.client_token) {
            session.generation = session.generation.saturating_add(1);
            if let Some(previous) = session.outbound.replace(request.outbound.clone()) {
                let _ignored = previous.try_send(Arc::new(ServerMessage::Bye {
                    reason: ByeReason::Replaced,
                }));
            }
            let lease = ConnectionLease {
                player_id: session.player_id,
                generation: session.generation,
            };
            let player_id = session.player_id;
            let events = self
                .game
                .reconnect_player(player_id)
                .map_err(|_| JoinError::RoomClosed)?;
            let resume = Arc::new(ServerMessage::Resume(self.game.resume_for(player_id)));
            if request.outbound.try_send(resume).is_err() {
                if let Some(session) = self.sessions.get_mut(&request.client_token) {
                    session.outbound = None;
                }
                let _ignored = self.game.disconnect_player(player_id, now);
                return Err(JoinError::OutboundUnavailable);
            }
            self.dispatch(events, now);
            self.ever_joined = true;
            self.empty_since = None;
            return Ok(JoinAccepted {
                lease,
                resumed: true,
            });
        }

        let claim_host = request.client_token == self.config.creator_token;
        let (player_id, events) = self.game.add_player(request.profile, claim_host)?;
        let lease = ConnectionLease {
            player_id,
            generation: 1,
        };
        let welcome = Arc::new(ServerMessage::Welcome(
            self.game.welcome_for(player_id, self.config.room_code),
        ));
        let outbound_available = request.outbound.try_send(welcome).is_ok();
        self.sessions.insert(
            request.client_token,
            Session {
                player_id,
                generation: 1,
                outbound: outbound_available.then_some(request.outbound),
            },
        );
        self.token_by_player.insert(player_id, request.client_token);
        self.ever_joined = true;
        if !outbound_available {
            let _ignored = self.game.disconnect_player(player_id, now);
            self.update_empty_deadline(now);
            return Err(JoinError::OutboundUnavailable);
        }
        self.empty_since = None;
        self.dispatch(events, now);
        Ok(JoinAccepted {
            lease,
            resumed: false,
        })
    }

    fn action(&mut self, lease: ConnectionLease, action: PlayerAction, now: GameTime) {
        if !self.is_current(lease) {
            return;
        }
        let kicked_target = match action {
            PlayerAction::KickPlayer { player_id } => Some(player_id),
            _ => None,
        };
        match self.game.apply(lease.player_id, action, now) {
            Ok(events) => {
                if let Some(target) = kicked_target {
                    self.ban_player(target);
                }
                self.dispatch(events, now);
            }
            Err(error) => self.send_error(lease.player_id, &error, now),
        }
    }

    fn leave(&mut self, lease: ConnectionLease, now: GameTime) {
        if !self.is_current(lease) {
            return;
        }
        if let Some(token) = self.token_by_player.get(&lease.player_id) {
            if let Some(session) = self.sessions.get_mut(token) {
                session.outbound = None;
            }
        }
        if let Ok(events) = self.game.disconnect_player(lease.player_id, now) {
            self.dispatch(events, now);
        }
    }

    fn ban_player(&mut self, player_id: PlayerId) {
        let Some(token) = self.token_by_player.remove(&player_id) else {
            return;
        };
        self.kicked.insert(token);
        if let Some(mut session) = self.sessions.remove(&token) {
            if let Some(outbound) = session.outbound.take() {
                let _ignored = outbound.try_send(Arc::new(ServerMessage::Bye {
                    reason: ByeReason::Kicked,
                }));
            }
        }
    }

    fn advance_deadlines(&mut self, now: GameTime) {
        for _ in 0..8 {
            if !self
                .game
                .next_deadline()
                .is_some_and(|deadline| deadline.is_due(now))
            {
                break;
            }
            match self.game.handle_deadline(now) {
                Ok(events) => self.dispatch(events, now),
                Err(error) => {
                    tracing::warn!(room = %self.config.room_code, %error, "room deadline failed");
                    break;
                }
            }
        }
        self.update_empty_deadline(now);
    }

    fn dispatch(&mut self, events: Vec<EmittedEvent>, now: GameTime) {
        let mut slow = HashSet::new();
        for event in events {
            let message = Arc::new(event.message);
            for session in self.sessions.values() {
                if !audience_includes(event.audience, session.player_id) {
                    continue;
                }
                let Some(outbound) = &session.outbound else {
                    continue;
                };
                if outbound.try_send(Arc::clone(&message)).is_err() {
                    slow.insert(session.player_id);
                }
            }
        }
        for player_id in slow {
            self.drop_slow_player(player_id, now);
        }
    }

    fn drop_slow_player(&mut self, player_id: PlayerId, now: GameTime) {
        if let Some(token) = self.token_by_player.get(&player_id) {
            if let Some(session) = self.sessions.get_mut(token) {
                session.outbound = None;
            }
        }
        if let Ok(events) = self.game.disconnect_player(player_id, now) {
            // Presence fanout is bounded too; a second slow client is handled on
            // the next actor iteration instead of recursing.
            for event in events {
                let message = Arc::new(event.message);
                for session in self.sessions.values() {
                    if audience_includes(event.audience, session.player_id) {
                        if let Some(outbound) = &session.outbound {
                            let _ignored = outbound.try_send(Arc::clone(&message));
                        }
                    }
                }
            }
        }
    }

    fn send_error(&mut self, player_id: PlayerId, error: &RuleError, now: GameTime) {
        self.dispatch(
            vec![EmittedEvent::player(
                player_id,
                ServerMessage::Error(error.as_protocol_error()),
            )],
            now,
        );
    }

    fn close_all(&mut self, reason: ByeReason) {
        let message = Arc::new(ServerMessage::Bye { reason });
        for session in self.sessions.values_mut() {
            if let Some(outbound) = session.outbound.take() {
                let _ignored = outbound.try_send(Arc::clone(&message));
            }
        }
    }

    fn is_current(&self, lease: ConnectionLease) -> bool {
        self.token_by_player
            .get(&lease.player_id)
            .and_then(|token| self.sessions.get(token))
            .is_some_and(|session| {
                session.generation == lease.generation && session.outbound.is_some()
            })
    }

    fn update_empty_deadline(&mut self, now: GameTime) {
        if self.game.connected_count() == 0 && self.ever_joined {
            self.empty_since.get_or_insert(now);
        } else {
            self.empty_since = None;
        }
    }

    fn lifecycle_expiry(&self, now: GameTime) -> Option<RoomExitReason> {
        if self.game.is_lobby()
            && GameDeadline::after(GameTime::default(), self.config.lobby_timeout).is_due(now)
        {
            return Some(RoomExitReason::LobbyExpired);
        }
        self.empty_since.and_then(|empty_since| {
            GameDeadline::after(empty_since, self.config.empty_timeout)
                .is_due(now)
                .then_some(RoomExitReason::Empty)
        })
    }

    fn next_wake(&self, now: GameTime) -> Instant {
        let mut deadline = self
            .game
            .is_lobby()
            .then(|| GameDeadline::after(GameTime::default(), self.config.lobby_timeout));
        if let Some(game_deadline) = self.game.next_deadline() {
            deadline = Some(deadline.map_or(game_deadline, |current| current.min(game_deadline)));
        }
        if let Some(empty_since) = self.empty_since {
            let empty_deadline = GameDeadline::after(empty_since, self.config.empty_timeout);
            deadline = Some(deadline.map_or(empty_deadline, |current| current.min(empty_deadline)));
        }
        let wake_millis = deadline.map_or_else(
            || now.saturating_add(Duration::from_secs(3_600)).0,
            |deadline| deadline.0.max(now.0),
        );
        self.started_at + Duration::from_millis(wake_millis)
    }

    fn now(&self) -> GameTime {
        GameTime::from_duration(self.started_at.elapsed())
    }
}

fn audience_includes(audience: Audience, player_id: PlayerId) -> bool {
    match audience {
        Audience::Everyone => true,
        Audience::Player(target) => target == player_id,
        Audience::EveryoneExcept(excluded) => excluded != player_id,
    }
}

fn unix_milliseconds() -> u64 {
    let duration = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration,
        Err(_) => Duration::ZERO,
    };
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
