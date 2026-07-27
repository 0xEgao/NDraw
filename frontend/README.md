# NDraw frontend

Canvas-first web client for the Rust NDraw game server. It uses Vinext/React,
DiceBear avatars, Phosphor icons, and locally bundled variable fonts.

## Run locally

```bash
npm install
npm run dev
```

The Rust HTTP and WebSocket server defaults to `http://127.0.0.1:3000`. When
the frontend uses that port, start the backend elsewhere and point the browser
client to it:

```bash
NEXT_PUBLIC_NDRAW_API_BASE=http://127.0.0.1:3001 npm run dev
```

Voice chat is optional and uses LiveKit. Set these variables on the Rust server:

```bash
LIVEKIT_URL=wss://your-project.livekit.cloud
LIVEKIT_API_KEY=...
LIVEKIT_API_SECRET=...
```

Without them the game still works; the voice button reports that voice chat is
not configured.

## Commands

- `npm run dev` starts the local frontend.
- `npm run lint` runs ESLint.
- `npm run build` creates the production worker bundle.
- `npm test` builds and verifies the TypeScript codec against Rust fixtures.

## Protocol notes

`lib/protocol.ts` mirrors `ndraw-proto` version 2. Opcode assignments are
explicit and the tests share the Rust hexadecimal fixtures. Lobby state, phase
changes, private word choices, chat, guesses, per-turn score summaries, drawing
ratings, freehand strokes, sampled shape strokes, erasing, undo, clear, and
reconnect snapshots are synchronized with the Rust server. Dedicated brush
identity, chat categories, and ordered point-seeded flood-fill operations are
mirrored in `ndraw-proto` and covered by shared hexadecimal fixtures.

For a manual second client during local testing:

```bash
node --experimental-strip-types scripts/live-test-client.ts \
  ws://127.0.0.1:3001/v1/ws/ROOM_CODE Guest
```
