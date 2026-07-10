//! Per-connection token buckets for client command classes.

use ndraw_proto::ClientMessage;
use tokio::time::Instant;

#[derive(Debug)]
pub(crate) struct ConnectionRateLimits {
    drawing: TokenBucket,
    text: TokenBucket,
    administrative: TokenBucket,
}

impl ConnectionRateLimits {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            drawing: TokenBucket::new(30.0, 60.0, now),
            text: TokenBucket::new(5.0, 10.0, now),
            administrative: TokenBucket::new(2.0, 2.0, now),
        }
    }

    pub(crate) fn allow(&mut self, message: &ClientMessage, now: Instant) -> bool {
        match message {
            ClientMessage::Draw(_) => self.drawing.allow(now),
            ClientMessage::Guess { .. } | ClientMessage::Chat { .. } => self.text.allow(now),
            ClientMessage::StartGame
            | ClientMessage::PickWord { .. }
            | ClientMessage::KickPlayer { .. }
            | ClientMessage::Rematch
            | ClientMessage::VoteDrawing { .. } => self.administrative.allow(now),
            ClientMessage::Hello(_) | ClientMessage::Pong { .. } => true,
        }
    }
}

#[derive(Debug)]
struct TokenBucket {
    refill_per_second: f64,
    capacity: f64,
    tokens: f64,
    updated_at: Instant,
}

impl TokenBucket {
    fn new(refill_per_second: f64, capacity: f64, now: Instant) -> Self {
        Self {
            refill_per_second,
            capacity,
            tokens: capacity,
            updated_at: now,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        let elapsed = match now.checked_duration_since(self.updated_at) {
            Some(duration) => duration.as_secs_f64(),
            None => 0.0,
        };
        self.tokens = (self.tokens + elapsed * self.refill_per_second).min(self.capacity);
        self.updated_at = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}
