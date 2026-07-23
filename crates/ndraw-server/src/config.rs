//! Environment-backed server configuration.

use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use thiserror::Error;

use crate::builtin_words;

/// Invalid environment configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Bind address could not be parsed.
    #[error("NDRAW_BIND must be a socket address: {0}")]
    BindAddress(#[source] std::net::AddrParseError),
    /// A numeric setting could not be parsed.
    #[error("{name} must be a positive integer: {value}")]
    InvalidInteger {
        /// Environment variable name.
        name: &'static str,
        /// Invalid input.
        value: String,
    },
    /// Heartbeat settings would close active clients before the next ping.
    #[error("NDRAW_INACTIVITY_TIMEOUT_SECONDS must exceed NDRAW_PING_INTERVAL_SECONDS")]
    InvalidHeartbeat,
    /// Embedded word data does not satisfy authoritative game constraints.
    #[error("built-in word catalogue is invalid: {0}")]
    InvalidWordCatalog(#[source] ndraw_den::WordDeckError),
}

/// Immutable process configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// TCP listener address.
    pub bind_address: SocketAddr,
    /// Base used in room-creation WebSocket URLs.
    pub public_ws_base_url: String,
    /// Explicit browser Origin allowlist. Empty permits all origins for local development.
    pub allowed_origins: Vec<String>,
    /// Capacity of each room actor mailbox.
    pub room_command_capacity: usize,
    /// Capacity of each socket writer queue.
    pub outbound_capacity: usize,
    /// Unstarted lobby lifetime.
    pub lobby_timeout: Duration,
    /// Empty-room grace period.
    pub empty_timeout: Duration,
    /// Time allowed for the first application `Hello`.
    pub hello_timeout: Duration,
    /// Application heartbeat interval.
    pub ping_interval: Duration,
    /// Maximum time without client traffic.
    pub inactivity_timeout: Duration,
    /// Maximum graceful-shutdown drain.
    pub shutdown_grace: Duration,
    /// Default word pool used by newly created rooms.
    pub words: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3_000),
            public_ws_base_url: "ws://127.0.0.1:3000".to_owned(),
            allowed_origins: Vec::new(),
            room_command_capacity: 1_024,
            outbound_capacity: 256,
            lobby_timeout: Duration::from_secs(180),
            empty_timeout: Duration::from_secs(60),
            hello_timeout: Duration::from_secs(10),
            ping_interval: Duration::from_secs(15),
            inactivity_timeout: Duration::from_secs(45),
            shutdown_grace: Duration::from_secs(10),
            words: builtin_words(),
        }
    }
}

impl ServerConfig {
    /// Loads supported settings from environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = Self::default();
        if let Ok(value) = env::var("NDRAW_BIND") {
            config.bind_address = value.parse().map_err(ConfigError::BindAddress)?;
        }
        if let Ok(value) = env::var("NDRAW_PUBLIC_WS_BASE_URL") {
            config.public_ws_base_url = value.trim_end_matches('/').to_owned();
        }
        if let Ok(value) = env::var("NDRAW_ALLOWED_ORIGINS") {
            config.allowed_origins = value
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect();
        }
        config.room_command_capacity =
            positive_usize("NDRAW_ROOM_COMMAND_CAPACITY", config.room_command_capacity)?;
        config.outbound_capacity =
            positive_usize("NDRAW_OUTBOUND_CAPACITY", config.outbound_capacity)?;
        config.lobby_timeout =
            duration_from_env("NDRAW_LOBBY_TIMEOUT_SECONDS", config.lobby_timeout)?;
        config.empty_timeout =
            duration_from_env("NDRAW_EMPTY_TIMEOUT_SECONDS", config.empty_timeout)?;
        config.hello_timeout =
            duration_from_env("NDRAW_HELLO_TIMEOUT_SECONDS", config.hello_timeout)?;
        config.ping_interval =
            duration_from_env("NDRAW_PING_INTERVAL_SECONDS", config.ping_interval)?;
        config.inactivity_timeout = duration_from_env(
            "NDRAW_INACTIVITY_TIMEOUT_SECONDS",
            config.inactivity_timeout,
        )?;
        config.shutdown_grace = Duration::from_secs(positive_u64(
            "NDRAW_SHUTDOWN_GRACE_SECONDS",
            config.shutdown_grace.as_secs(),
        )?);
        if config.inactivity_timeout <= config.ping_interval {
            return Err(ConfigError::InvalidHeartbeat);
        }
        ndraw_den::WordDeck::new(
            config.words.clone(),
            usize::from(ndraw_proto::limit::MAX_WORD_CHOICES),
            [0; 32],
        )
        .map_err(ConfigError::InvalidWordCatalog)?;
        Ok(config)
    }
}

fn positive_usize(name: &'static str, default: usize) -> Result<usize, ConfigError> {
    let Ok(value) = env::var(name) else {
        return Ok(default);
    };
    value
        .parse::<usize>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or(ConfigError::InvalidInteger { name, value })
}

fn positive_u64(name: &'static str, default: u64) -> Result<u64, ConfigError> {
    let Ok(value) = env::var(name) else {
        return Ok(default);
    };
    value
        .parse::<u64>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or(ConfigError::InvalidInteger { name, value })
}

fn duration_from_env(name: &'static str, default: Duration) -> Result<Duration, ConfigError> {
    positive_u64(name, default.as_secs()).map(Duration::from_secs)
}
