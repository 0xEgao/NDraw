# ndraw-server

`ndraw-server` is NDraw's network edge. It exposes a small HTTP control plane
and adapts binary WebSocket connections to the single-owner room actors in
`ndraw-den`.

## Run locally

```console
cargo run -p ndraw-server
```

The default listener is `127.0.0.1:3000`.

## HTTP API

- `POST /v1/rooms` creates an ephemeral room.
- `GET /v1/ws/{room_code}` upgrades to the binary protocol. The first
  application frame must be `ClientMessage::Hello`.
- `GET /healthz` reports process liveness.
- `GET /readyz` reports whether new rooms and connections are accepted.
- `GET /metrics` exposes Prometheus text metrics.

Room creation accepts:

```json
{
  "client_token": "550e8400-e29b-41d4-a716-446655440000",
  "settings": {
    "rounds": 3,
    "draw_seconds": 100,
    "word_choices": 4,
    "max_players": 12
  }
}
```

Drawing, guesses, chat, presence, and game commands are intentionally not
available through HTTP.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `NDRAW_BIND` | `127.0.0.1:3000` | Listener socket address |
| `NDRAW_PUBLIC_WS_BASE_URL` | `ws://127.0.0.1:3000` | Base URL returned by room creation |
| `NDRAW_ALLOWED_ORIGINS` | empty | Comma-separated browser Origin allowlist; empty permits local development |
| `NDRAW_ROOM_COMMAND_CAPACITY` | `1024` | Commands buffered per room actor |
| `NDRAW_OUTBOUND_CAPACITY` | `256` | Messages buffered per connection writer |
| `NDRAW_LOBBY_TIMEOUT_SECONDS` | `180` | Unstarted room lifetime |
| `NDRAW_EMPTY_TIMEOUT_SECONDS` | `60` | Empty-room grace period |
| `NDRAW_HELLO_TIMEOUT_SECONDS` | `10` | First-message deadline |
| `NDRAW_PING_INTERVAL_SECONDS` | `15` | Application heartbeat interval |
| `NDRAW_INACTIVITY_TIMEOUT_SECONDS` | `45` | Silent-client timeout |
| `NDRAW_SHUTDOWN_GRACE_SECONDS` | `10` | Maximum room drain time |

Production deployments should terminate TLS at a trusted reverse proxy or
load balancer, set `NDRAW_PUBLIC_WS_BASE_URL` to the public `wss://` origin,
and configure an explicit Origin allowlist.

## State and shutdown

Rooms and anonymous reconnect records live only in memory. Graceful shutdown
marks readiness unavailable, stops accepting rooms and WebSockets, sends room
shutdown notifications, and waits for a bounded actor drain. Process restarts
do not preserve active games by design.

## Word data

Human games use the embedded `src/data/words-easy.txt`,
`words-medium.txt`, and `words-hard.txt` catalogues. They are compiled into the
binary, normalized, and deduplicated at startup. The `words-bot*.txt` files are
reserved for a future drawing-bot mode and are not included in normal rooms.
