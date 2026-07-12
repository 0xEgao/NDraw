//! HTTP and WebSocket transport layer for the NDraw authoritative game engine.
//!
//! This crate deliberately keeps network concerns outside `ndraw-den`. HTTP
//! creates and discovers rooms, while a binary WebSocket carries all gameplay.

#![forbid(unsafe_code)]

mod config;
mod data;
mod directory;
mod http;
mod metrics;
mod rate_limit;
mod ws;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub use config::{ConfigError, ServerConfig};
pub use data::builtin_words;
pub use directory::{CreateRoomError, CreatedRoom, RoomDirectory};
pub use http::{CreateRoomRequest, CreateRoomResponse};
pub use metrics::ServerMetrics;

/// Shared application services used by Axum handlers.
#[derive(Debug)]
pub struct AppState {
    config: ServerConfig,
    directory: RoomDirectory,
    metrics: ServerMetrics,
    ready: AtomicBool,
}

impl AppState {
    /// Constructs an initially ready server state.
    #[must_use]
    pub fn new(config: ServerConfig) -> Self {
        let metrics = ServerMetrics::default();
        let directory = RoomDirectory::new(config.clone(), metrics.clone());
        Self {
            config,
            directory,
            metrics,
            ready: AtomicBool::new(true),
        }
    }

    /// Returns immutable process configuration.
    #[must_use]
    pub const fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Returns the local room directory.
    #[must_use]
    pub const fn directory(&self) -> &RoomDirectory {
        &self.directory
    }

    /// Returns the telemetry registry.
    #[must_use]
    pub const fn metrics(&self) -> &ServerMetrics {
        &self.metrics
    }

    /// Whether this process accepts new rooms and WebSocket sessions.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Stops new work, notifies every room, and waits for a bounded drain.
    pub async fn shutdown(&self) {
        self.ready.store(false, Ordering::Release);
        self.directory
            .shutdown_all(self.config.shutdown_grace)
            .await;
    }
}

/// Builds the complete HTTP application without binding a socket.
pub fn build_router(state: Arc<AppState>) -> axum::Router {
    http::router(state)
}
