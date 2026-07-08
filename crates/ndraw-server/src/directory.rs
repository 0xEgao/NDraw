//! In-process room lookup and actor lifecycle management.

use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dashmap::{DashMap, mapref::entry::Entry};
use ndraw_den::{RoomConfig, RoomHandle, spawn_room};
use ndraw_proto::{ClientToken, RoomCode, RoomSettings, limit::ROOM_CODE_ALPHABET};
use rand::{Rng, RngCore};
use thiserror::Error;

use crate::{ServerConfig, ServerMetrics};

const ROOM_CODE_ATTEMPTS: usize = 64;

/// Result of reserving and starting a local room.
#[derive(Debug, Clone)]
pub struct CreatedRoom {
    /// Public room identity.
    pub room_code: RoomCode,
    /// Actor command endpoint.
    pub handle: RoomHandle,
    /// Unix timestamp when an unstarted lobby expires.
    pub lobby_expires_at_ms: u64,
}

/// Room creation failure.
#[derive(Debug, Error)]
pub enum CreateRoomError {
    /// Process has started graceful shutdown.
    #[error("server is not accepting new rooms")]
    Unavailable,
    /// Generated codes repeatedly collided with active rooms.
    #[error("could not allocate a unique room code")]
    CodeSpaceExhausted,
    /// Settings or the configured word pool could not construct a game.
    #[error("room configuration is invalid: {0}")]
    InvalidGame(#[from] ndraw_den::GameBuildError),
    /// Internal room-code generation violated the shared alphabet.
    #[error("generated an invalid room code")]
    InvalidGeneratedCode,
}

#[derive(Debug, Clone)]
struct DirectoryEntry {
    generation: u64,
    handle: Option<RoomHandle>,
}

#[derive(Debug)]
struct DirectoryInner {
    rooms: DashMap<RoomCode, DirectoryEntry>,
    next_generation: AtomicU64,
    accepting: AtomicBool,
    lifecycle_gate: RwLock<()>,
    config: ServerConfig,
    metrics: ServerMetrics,
}

/// Cloneable in-process implementation of room lookup.
#[derive(Debug, Clone)]
pub struct RoomDirectory {
    inner: Arc<DirectoryInner>,
}

impl RoomDirectory {
    /// Constructs an empty local directory.
    #[must_use]
    pub fn new(config: ServerConfig, metrics: ServerMetrics) -> Self {
        Self {
            inner: Arc::new(DirectoryInner {
                rooms: DashMap::new(),
                next_generation: AtomicU64::new(1),
                accepting: AtomicBool::new(true),
                lifecycle_gate: RwLock::new(()),
                config,
                metrics,
            }),
        }
    }

    /// Finds one currently live room actor.
    #[must_use]
    pub fn get(&self, room_code: RoomCode) -> Option<RoomHandle> {
        self.inner
            .rooms
            .get(&room_code)
            .and_then(|entry| entry.handle.clone())
    }

    /// Creates, registers, and supervises one actor.
    pub fn create(
        &self,
        creator_token: ClientToken,
        settings: RoomSettings,
    ) -> Result<CreatedRoom, CreateRoomError> {
        let _gate = match self.inner.lifecycle_gate.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.inner.accepting.load(Ordering::Acquire) {
            return Err(CreateRoomError::Unavailable);
        }
        let generation = self.inner.next_generation.fetch_add(1, Ordering::Relaxed);
        let room_code = self.allocate_and_reserve(generation)?;
        let mut random_seed = [0_u8; 32];
        rand::rng().fill_bytes(&mut random_seed);
        let room = match spawn_room(RoomConfig {
            room_code,
            room_generation: generation,
            creator_token,
            settings,
            words: self.inner.config.words.clone(),
            random_seed,
            command_capacity: self.inner.config.room_command_capacity,
            lobby_timeout: self.inner.config.lobby_timeout,
            empty_timeout: self.inner.config.empty_timeout,
            exit_tx: None,
        }) {
            Ok(room) => room,
            Err(error) => {
                self.remove_generation(room_code, generation);
                return Err(CreateRoomError::InvalidGame(error));
            }
        };
        let handle = room.handle.clone();
        if let Some(mut entry) = self.inner.rooms.get_mut(&room_code) {
            if entry.generation == generation {
                entry.handle = Some(handle.clone());
            }
        }
        self.inner.metrics.room_created();

        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let outcome = room.task.await;
            if let Some(inner) = weak.upgrade() {
                let should_remove = inner
                    .rooms
                    .get(&room_code)
                    .is_some_and(|entry| entry.generation == generation);
                if should_remove {
                    inner.rooms.remove(&room_code);
                    inner.metrics.room_closed();
                }
                match outcome {
                    Ok(reason) => tracing::debug!(%room_code, ?reason, "room actor stopped"),
                    Err(error) => tracing::error!(%room_code, %error, "room actor failed"),
                }
            }
        });

        Ok(CreatedRoom {
            room_code,
            handle,
            lobby_expires_at_ms: unix_milliseconds()
                .saturating_add(duration_milliseconds(self.inner.config.lobby_timeout)),
        })
    }

    /// Requests all actors to stop and waits until the directory drains or times out.
    pub async fn shutdown_all(&self, grace: Duration) {
        let handles: Vec<RoomHandle> = {
            let _gate = match self.inner.lifecycle_gate.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            self.inner.accepting.store(false, Ordering::Release);
            self.inner
                .rooms
                .iter()
                .filter_map(|entry| entry.handle.clone())
                .collect()
        };
        for handle in handles {
            let _ignored = handle.try_shutdown();
        }

        let wait = async {
            while !self.inner.rooms.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        if tokio::time::timeout(grace, wait).await.is_err() {
            tracing::warn!(remaining = self.inner.rooms.len(), "room drain timed out");
        }
    }

    fn allocate_and_reserve(&self, generation: u64) -> Result<RoomCode, CreateRoomError> {
        let mut rng = rand::rng();
        for _ in 0..ROOM_CODE_ATTEMPTS {
            let mut bytes = [0_u8; ndraw_proto::limit::ROOM_CODE_LENGTH];
            for byte in &mut bytes {
                let index = rng.random_range(0..ROOM_CODE_ALPHABET.len());
                *byte = ROOM_CODE_ALPHABET[index];
            }
            let code = RoomCode::new(bytes).map_err(|_| CreateRoomError::InvalidGeneratedCode)?;
            if let Entry::Vacant(entry) = self.inner.rooms.entry(code) {
                entry.insert(DirectoryEntry {
                    generation,
                    handle: None,
                });
                return Ok(code);
            }
        }
        Err(CreateRoomError::CodeSpaceExhausted)
    }

    fn remove_generation(&self, room_code: RoomCode, generation: u64) {
        let should_remove = self
            .inner
            .rooms
            .get(&room_code)
            .is_some_and(|entry| entry.generation == generation);
        if should_remove {
            self.inner.rooms.remove(&room_code);
        }
    }
}

fn unix_milliseconds() -> u64 {
    let duration = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration,
        Err(_) => Duration::ZERO,
    };
    duration_milliseconds(duration)
}

fn duration_milliseconds(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
