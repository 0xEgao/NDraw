//! Connection identities used to fence stale WebSocket tasks.

use ndraw_proto::PlayerId;

/// Capability identifying exactly one active connection generation.
///
/// A browser reconnect keeps its [`PlayerId`] but receives a larger
/// generation. Reader tasks must include this lease with every action and
/// disconnect notification. The room actor ignores notifications from an old
/// lease, preventing a replaced socket from disconnecting its successor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionLease {
    /// Stable player identity retained for the lifetime of the room.
    pub player_id: PlayerId,
    /// Monotonically increasing connection generation for that player.
    pub generation: u64,
}
