# En Parlant~ Multiplayer Relay

A lightweight WebSocket relay server for [En Parlant~](https://github.com/DarrellThomas/en-parlant) multiplayer chess. Written in Rust using [Axum](https://github.com/tokio-rs/axum) and [socketioxide](https://github.com/Totodore/socketioxide).

The relay connects two players for real-time chess over Socket.IO. It holds rooms in memory, forwards moves between players, and does nothing else. No game logic, no move validation, no database, no persistent storage.

## Architecture

```
Player A (host)                    Relay Server                    Player B (joiner)
     |                                  |                                |
     |--- create_game(name) ----------->|                                |
     |<-- game_created(code) -----------|                                |
     |                                  |<--- join_game(code, name) -----|
     |<-- peer_joined(name) -----------|--- game_joined(color, name) -->|
     |                                  |                                |
     |--- game_move(uci, times) ------->|--- game_move(uci, times) --->|
     |<-- game_move(uci, times) --------|<-- game_move(uci, times) ----|
     |                                  |                                |
     |--- heartbeat ------------------->|--- peer_heartbeat ----------->|
```

The relay is a thin forwarding layer. Clients are authoritative -- the server never inspects or validates chess moves.

## Quick Start

### Run locally

```bash
git clone https://github.com/DarrellThomas/en-parlant-relay.git
cd en-parlant-relay
cargo run
```

The server starts on port **3210**. Point En Parlant~ at `ws://localhost:3210`.

### Deploy to Fly.io

The repository includes a `fly.toml` configuration. Fly.io's free tier is suitable for personal use.

```bash
# Install Fly CLI
curl -L https://fly.io/install.sh | sh

# Log in and deploy
fly auth login
fly launch
fly deploy
```

Fly.io builds the Rust binary via the included `Dockerfile`, deploys it, and gives you a public URL like `your-app-name.fly.dev`. Point En Parlant~ at `wss://your-app-name.fly.dev`.

### Build from source

```bash
cargo build --release
./target/release/en-parlant-relay
```

For production, run behind a reverse proxy (nginx, Caddy) to handle TLS. The relay speaks plain WebSocket -- your proxy adds the `wss://` layer.

## Socket.IO Events

### Client → Server

| Event | Payload | Description |
|-------|---------|-------------|
| `create_game` | `{ name }` | Host creates a room and gets a 6-character code |
| `join_game` | `{ code, name }` | Joiner enters a room by code |
| `game_move` | `{ uci, whiteTime?, blackTime? }` | Forward a move to the opponent |
| `resign` | `{ color }` | Resign the game |
| `offer_draw` | `{}` | Offer a draw |
| `accept_draw` | `{}` | Accept a draw offer |
| `ready` | `{}` | Signal readiness for rematch |
| `heartbeat` | `{}` | Keep-alive ping (every 5 seconds) |

### Server → Client

| Event | Payload | Description |
|-------|---------|-------------|
| `game_created` | `{ code }` | Room created, here's the share code |
| `game_joined` | `{ color, peerName }` | You joined the room as this color |
| `peer_joined` | `{ peerName }` | Your opponent has joined |
| `game_move` | `{ uci, whiteTime?, blackTime? }` | Opponent's move |
| `resign` | `{ color }` | Opponent resigned |
| `offer_draw` | `{}` | Opponent offers a draw |
| `accept_draw` | `{}` | Opponent accepted the draw |
| `peer_ready` | `{}` | Opponent is ready for a rematch |
| `peer_heartbeat` | `{}` | Opponent is still connected |
| `heartbeat_ack` | `{}` | Server acknowledges your heartbeat |
| `peer_left` | `{}` | Opponent disconnected |
| `error` | `{ message }` | Error (game not found, game full) |

## Room Codes

Codes are 6 characters formatted as `XX-XX-XX` for easy verbal sharing. The character set excludes visually ambiguous characters:

- No `0` or `O` (zero vs. letter O)
- No `1`, `I`, or `L` (one vs. capital I vs. capital L)

## Game Rules

- **Host plays White, joiner plays Black.** Determined by join order.
- **Two players per room.** A third connection is rejected.
- **Rooms expire after 30 minutes of inactivity.** A cleanup task sweeps idle rooms every 60 seconds.
- **No persistent storage.** Server restart clears all rooms. This is by design.

## Resource Requirements

The relay is extremely lightweight. It holds a `HashMap` of rooms in memory and forwards JSON events between two Socket.IO connections. A small VPS, a Raspberry Pi, or Fly.io's smallest instance can handle it comfortably.

The included `fly.toml` requests:

- `shared-cpu-1x` (shared vCPU)
- `256mb` memory
- 1 machine minimum, auto-stop when idle

## Configuring En Parlant~

1. Open **Settings** in En Parlant~
2. Find the multiplayer relay server URL setting
3. Enter your server's WebSocket endpoint:
   - Fly.io: `wss://your-app-name.fly.dev`
   - Self-hosted with TLS proxy: `wss://relay.yourdomain.com`
   - Local development: `ws://localhost:3210`

## Why Self-Host?

The default relay works out of the box, but you might want your own for:

- **Privacy** -- all game traffic stays on your infrastructure
- **Lower latency** -- deploy closer to your players
- **Independence** -- no dependency on the default relay's uptime

## License

MIT. See [LICENSE](LICENSE) for details.
