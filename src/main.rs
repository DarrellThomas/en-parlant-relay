// Copyright (c) 2026 Darrell Thomas
// Licensed under the MIT License. See LICENSE file in the project root for details.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::routing::get;
use rand::Rng;
use serde::{Deserialize, Serialize};
use socketioxide::extract::{Data, SocketRef, State};
use socketioxide::SocketIo;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::info;

/// Charset without ambiguous chars (no 0/O, 1/I/L)
const CODE_CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 6;
const ROOM_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug)]
struct Room {
    creator_name: String,
    player_count: u8,
    #[allow(dead_code)]
    created_at: Instant,
    last_activity: Instant,
}

/// Shared state: rooms by code + reverse map from socket ID to room code
#[derive(Debug, Default)]
struct RelayState {
    rooms: HashMap<String, Room>,
    sid_to_room: HashMap<String, String>,
}

type SharedState = Arc<Mutex<RelayState>>;

fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let chars: Vec<u8> = (0..CODE_LEN)
        .map(|_| CODE_CHARS[rng.gen_range(0..CODE_CHARS.len())])
        .collect();
    let s = String::from_utf8(chars).unwrap();
    format!("{}-{}-{}", &s[0..2], &s[2..4], &s[4..6])
}

#[derive(Deserialize)]
struct CreateGame {
    name: String,
}

#[derive(Serialize)]
struct GameCreated {
    code: String,
}

#[derive(Deserialize)]
struct JoinGame {
    code: String,
    name: String,
}

#[derive(Serialize)]
struct GameJoined {
    color: String,
    #[serde(rename = "peerName")]
    peer_name: String,
}

#[derive(Serialize)]
struct PeerJoined {
    #[serde(rename = "peerName")]
    peer_name: String,
}

#[derive(Deserialize, Serialize, Clone)]
struct GameMove {
    uci: String,
    #[serde(rename = "whiteTime")]
    white_time: Option<f64>,
    #[serde(rename = "blackTime")]
    black_time: Option<f64>,
}

#[derive(Deserialize, Serialize, Clone)]
struct Resign {
    color: String,
}

fn on_connect(socket: SocketRef, _state: State<SharedState>) {
    let sid = socket.id.to_string();
    info!("Client connected: {}", sid);

    socket.on(
        "create_game",
        |socket: SocketRef, Data::<CreateGame>(data), state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let code = generate_code();
            info!("{} creating game {}", sid, code);

            let room = Room {
                creator_name: data.name,
                player_count: 1,
                created_at: Instant::now(),
                last_activity: Instant::now(),
            };

            let mut s = state.lock().await;
            s.rooms.insert(code.clone(), room);
            s.sid_to_room.insert(sid, code.clone());
            drop(s);

            let _ = socket.leave_all();
            let _ = socket.join(code.clone());
            socket.emit("game_created", &GameCreated { code }).ok();
        },
    );

    socket.on(
        "join_game",
        |socket: SocketRef, Data::<JoinGame>(data), state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let code = data.code.to_uppercase();
            info!("{} joining game {}", sid, code);

            let mut s = state.lock().await;
            let room = match s.rooms.get_mut(&code) {
                Some(r) => r,
                None => {
                    socket
                        .emit("error", &serde_json::json!({"message": "Game not found"}))
                        .ok();
                    return;
                }
            };

            if room.player_count >= 2 {
                socket
                    .emit("error", &serde_json::json!({"message": "Game is full"}))
                    .ok();
                return;
            }

            room.player_count = 2;
            room.last_activity = Instant::now();
            let creator_name = room.creator_name.clone();
            s.sid_to_room.insert(sid, code.clone());
            drop(s);

            let _ = socket.leave_all();
            let _ = socket.join(code.clone());

            // Joiner gets black
            socket
                .emit(
                    "game_joined",
                    &GameJoined {
                        color: "black".to_string(),
                        peer_name: creator_name,
                    },
                )
                .ok();

            // Notify creator
            socket
                .to(code)
                .emit(
                    "peer_joined",
                    &PeerJoined {
                        peer_name: data.name,
                    },
                )
                .ok();
        },
    );

    // Forward game_move to peer
    socket.on(
        "game_move",
        |socket: SocketRef, Data::<GameMove>(data), state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let mut s = state.lock().await;
            if let Some(code) = s.sid_to_room.get(&sid).cloned() {
                if let Some(room) = s.rooms.get_mut(&code) {
                    room.last_activity = Instant::now();
                }
                drop(s);
                socket.to(code).emit("game_move", &data).ok();
            }
        },
    );

    // Forward resign to peer
    socket.on(
        "resign",
        |socket: SocketRef, Data::<Resign>(data), state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let s = state.lock().await;
            if let Some(code) = s.sid_to_room.get(&sid).cloned() {
                drop(s);
                socket.to(code).emit("resign", &data).ok();
            }
        },
    );

    // Forward draw offer to peer
    socket.on(
        "offer_draw",
        |socket: SocketRef, state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let s = state.lock().await;
            if let Some(code) = s.sid_to_room.get(&sid).cloned() {
                drop(s);
                socket
                    .to(code)
                    .emit("offer_draw", &serde_json::json!({}))
                    .ok();
            }
        },
    );

    // Forward draw acceptance to peer
    socket.on(
        "accept_draw",
        |socket: SocketRef, state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let s = state.lock().await;
            if let Some(code) = s.sid_to_room.get(&sid).cloned() {
                drop(s);
                socket
                    .to(code)
                    .emit("accept_draw", &serde_json::json!({}))
                    .ok();
            }
        },
    );

    // Forward ready signal to peer
    socket.on(
        "ready",
        |socket: SocketRef, state: State<SharedState>| async move {
            let sid = socket.id.to_string();
            let s = state.lock().await;
            if let Some(code) = s.sid_to_room.get(&sid).cloned() {
                drop(s);
                socket
                    .to(code)
                    .emit("peer_ready", &serde_json::json!({}))
                    .ok();
            }
        },
    );

    // Heartbeat: ack sender, forward to peer
    socket.on(
        "heartbeat",
        |socket: SocketRef, state: State<SharedState>| async move {
            socket.emit("heartbeat_ack", &serde_json::json!({})).ok();
            let sid = socket.id.to_string();
            let s = state.lock().await;
            if let Some(code) = s.sid_to_room.get(&sid).cloned() {
                drop(s);
                socket
                    .to(code)
                    .emit("peer_heartbeat", &serde_json::json!({}))
                    .ok();
            }
        },
    );

    // Handle disconnect: notify peer and clean up
    let state_clone = _state.0.clone();
    socket.on_disconnect(move |socket: SocketRef| async move {
        let sid = socket.id.to_string();
        info!("Client disconnected: {}", sid);

        let mut s = state_clone.lock().await;
        if let Some(code) = s.sid_to_room.remove(&sid) {
            s.rooms.remove(&code);
            drop(s);
            socket
                .to(code)
                .emit("peer_left", &serde_json::json!({}))
                .ok();
        }
    });
}

async fn cleanup_stale_rooms(state: SharedState) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut s = state.lock().await;
        let before = s.rooms.len();

        // Collect stale room codes
        let stale: Vec<String> = s
            .rooms
            .iter()
            .filter(|(_, room)| room.last_activity.elapsed() > ROOM_TIMEOUT)
            .map(|(code, _)| code.clone())
            .collect();

        // Remove stale rooms and their SID mappings
        for code in &stale {
            s.rooms.remove(code);
            s.sid_to_room.retain(|_, v| v != code);
        }

        let removed = before - s.rooms.len();
        if removed > 0 {
            info!(
                "Cleaned up {} stale rooms, {} remaining",
                removed,
                s.rooms.len()
            );
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state: SharedState = Arc::new(Mutex::new(RelayState::default()));

    let (layer, io) = SocketIo::builder()
        .with_state(state.clone())
        .ping_interval(Duration::from_secs(5))
        .ping_timeout(Duration::from_secs(5))
        .build_layer();

    io.ns("/", on_connect);

    tokio::spawn(cleanup_stale_rooms(state));

    let app = axum::Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(layer)
        .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:3210";
    info!("En Parlant~ relay server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
