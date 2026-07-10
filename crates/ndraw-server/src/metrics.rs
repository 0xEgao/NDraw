//! Small process-local Prometheus registry.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Lock-free counters and gauges exposed by `/metrics`.
#[derive(Debug, Clone, Default)]
pub struct ServerMetrics {
    inner: Arc<MetricsInner>,
}

#[derive(Debug, Default)]
struct MetricsInner {
    active_rooms: AtomicU64,
    active_sockets: AtomicU64,
    connected_players: AtomicU64,
    rooms_created: AtomicU64,
    incoming_messages: AtomicU64,
    outgoing_messages: AtomicU64,
    incoming_bytes: AtomicU64,
    outgoing_bytes: AtomicU64,
    decode_failures: AtomicU64,
    rate_limit_rejections: AtomicU64,
    slow_client_disconnects: AtomicU64,
}

impl ServerMetrics {
    pub(crate) fn room_created(&self) {
        self.inner.active_rooms.fetch_add(1, Ordering::Relaxed);
        self.inner.rooms_created.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn room_closed(&self) {
        decrement(&self.inner.active_rooms);
    }

    pub(crate) fn socket_connected(&self) {
        self.inner.active_sockets.fetch_add(1, Ordering::Relaxed);
        self.inner.connected_players.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn socket_disconnected(&self) {
        decrement(&self.inner.active_sockets);
        decrement(&self.inner.connected_players);
    }

    pub(crate) fn incoming(&self, bytes: usize) {
        self.inner.incoming_messages.fetch_add(1, Ordering::Relaxed);
        self.inner
            .incoming_bytes
            .fetch_add(saturating_usize(bytes), Ordering::Relaxed);
    }

    pub(crate) fn outgoing(&self, bytes: usize) {
        self.inner.outgoing_messages.fetch_add(1, Ordering::Relaxed);
        self.inner
            .outgoing_bytes
            .fetch_add(saturating_usize(bytes), Ordering::Relaxed);
    }

    pub(crate) fn decode_failure(&self) {
        self.inner.decode_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn rate_limit_rejection(&self) {
        self.inner
            .rate_limit_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn slow_client_disconnect(&self) {
        self.inner
            .slow_client_disconnects
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the current active-room gauge.
    #[must_use]
    pub fn active_rooms(&self) -> u64 {
        self.inner.active_rooms.load(Ordering::Relaxed)
    }

    /// Renders the Prometheus text exposition format.
    #[must_use]
    pub fn render(&self) -> String {
        let metrics = [
            ("ndraw_active_rooms", &self.inner.active_rooms, "gauge"),
            ("ndraw_active_sockets", &self.inner.active_sockets, "gauge"),
            (
                "ndraw_connected_players",
                &self.inner.connected_players,
                "gauge",
            ),
            (
                "ndraw_rooms_created_total",
                &self.inner.rooms_created,
                "counter",
            ),
            (
                "ndraw_incoming_messages_total",
                &self.inner.incoming_messages,
                "counter",
            ),
            (
                "ndraw_outgoing_messages_total",
                &self.inner.outgoing_messages,
                "counter",
            ),
            (
                "ndraw_incoming_bytes_total",
                &self.inner.incoming_bytes,
                "counter",
            ),
            (
                "ndraw_outgoing_bytes_total",
                &self.inner.outgoing_bytes,
                "counter",
            ),
            (
                "ndraw_decode_failures_total",
                &self.inner.decode_failures,
                "counter",
            ),
            (
                "ndraw_rate_limit_rejections_total",
                &self.inner.rate_limit_rejections,
                "counter",
            ),
            (
                "ndraw_slow_client_disconnects_total",
                &self.inner.slow_client_disconnects,
                "counter",
            ),
        ];
        let mut output = String::new();
        for (name, value, metric_type) in metrics {
            output.push_str("# TYPE ");
            output.push_str(name);
            output.push(' ');
            output.push_str(metric_type);
            output.push('\n');
            output.push_str(name);
            output.push(' ');
            output.push_str(&value.load(Ordering::Relaxed).to_string());
            output.push('\n');
        }
        output
    }
}

fn decrement(value: &AtomicU64) {
    let _previous = value.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(1))
    });
}

fn saturating_usize(value: usize) -> u64 {
    value.min(u64::MAX as usize) as u64
}
