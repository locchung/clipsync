//! # ClipSync Relay Server
//!
//! This is the cloud relay server that helps devices sync when they're not on
//! the same local network.
//!
//! ## Rust Learning: Building a Web Server with Axum
//!
//! Axum is a modern Rust web framework built on top of Tokio (async runtime)
//! and Tower (middleware). Key concepts:
//!
//! 1. **Router**: Defines URL routes and what handles them
//! 2. **Handler**: Async functions that process requests
//! 3. **State**: Shared data accessible to all handlers
//! 4. **Middleware**: Code that runs before/after handlers (logging, auth, etc.)
//!
//! ## Architecture
//!
//! The server maintains "rooms" - groups of paired devices that share clipboards.
//! When a device sends a ClipboardUpdate, the server broadcasts it to all other
//! devices in the same room.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use clipsync_core::protocol::Message;
use clipsync_core::DeviceId;

// ============================================================================
// APP STATE
// ============================================================================
/// Shared state for the entire application.
///
/// ## Rust Concept: Arc<T> for Shared State
///
/// Web servers handle many requests concurrently. They all need access to
/// the same data (rooms, connected devices, etc.).
///
/// `Arc` (Atomic Reference Counted) lets multiple tasks share ownership.
/// When you `clone()` an Arc, you get another pointer to the SAME data.
/// The data is only freed when ALL Arcs are dropped.
///
/// ## Rust Concept: RwLock for Interior Mutability
///
/// Normally, you need `&mut` to modify data. But with shared ownership (Arc),
/// everyone has `&` (immutable reference). `RwLock` provides "interior mutability":
/// - `read().await`: Get shared read access (many readers allowed)
/// - `write().await`: Get exclusive write access (one writer, no readers)
#[derive(Clone)]
pub struct AppState {
    /// All active rooms
    rooms: Arc<RwLock<HashMap<RoomId, Room>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

// ============================================================================
// ROOM MANAGEMENT
// ============================================================================
/// A unique identifier for a room (join code).
///
/// Rooms are identified by a 6-character alphanumeric code like "ABC123".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(String);

impl RoomId {
    /// Generates a new random room ID.
    fn generate() -> Self {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // Avoid confusing chars
        let mut rng = rand::thread_rng();
        let code: String = (0..6)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        Self(code)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// A room is a group of paired devices that share clipboards.
#[derive(Debug)]
pub struct Room {
    id: RoomId,
    /// Broadcast channel for sending messages to all connected devices
    ///
    /// ## Rust Concept: Broadcast Channels
    ///
    /// `broadcast::channel` creates a multi-producer, multi-consumer channel.
    /// When you `send()`, ALL receivers get a copy of the message.
    /// This is perfect for "chat room" style communication.
    tx: broadcast::Sender<Message>,
    /// Connected devices (device_id -> device_name)
    devices: HashMap<DeviceId, String>,
}

impl Room {
    fn new(id: RoomId) -> Self {
        // Create a broadcast channel with buffer size 100
        let (tx, _) = broadcast::channel(100);
        Self {
            id,
            tx,
            devices: HashMap::new(),
        }
    }
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================
/// ## Rust Concept: `#[tokio::main]`
///
/// Async functions need a "runtime" to execute. `#[tokio::main]` is a macro
/// that sets up the Tokio runtime and runs your async main function.
///
/// Under the hood, it generates code like:
/// ```rust
/// fn main() {
///     tokio::runtime::Runtime::new().unwrap().block_on(async_main())
/// }
/// ```
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("clipsync_server=debug".parse()?)
                .add_directive("tower_http=debug".parse()?),
        )
        .init();

    info!("Starting ClipSync relay server...");

    // Create shared state
    let state = AppState::new();

    // ## Rust Concept: Building a Router
    //
    // Axum uses a builder pattern for routes:
    // - `route("/path", handler)`: Add a route
    // - `get(handler)`: HTTP GET method
    // - `post(handler)`: HTTP POST method
    // - `.with_state(state)`: Make state available to handlers
    let app = Router::new()
        // Health check endpoint
        .route("/health", get(health_check))
        // Create a new room
        .route("/rooms", post(create_room))
        // WebSocket connection for a room
        .route("/rooms/:room_id/ws", get(websocket_handler))
        // Get room info
        .route("/rooms/:room_id", get(get_room_info))
        // Add logging middleware
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // Add CORS for web clients
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        // Attach shared state
        .with_state(state);

    // Start the server
    let addr = "0.0.0.0:3002";
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// HTTP HANDLERS
// ============================================================================
/// Health check endpoint - returns 200 OK if server is running.
///
/// ## Rust Concept: Axum Handlers
///
/// Handlers are async functions that:
/// 1. Take "extractors" as arguments (State, Path, Json, etc.)
/// 2. Return something that implements `IntoResponse`
///
/// Axum automatically extracts data from the request based on types.
async fn health_check() -> &'static str {
    "OK"
}

/// Response for room creation.
#[derive(Serialize)]
struct CreateRoomResponse {
    room_id: String,
}

/// Creates a new room and returns the join code.
///
/// ## Rust Concept: `Json` Extractor/Response
///
/// - `Json<T>` as argument: Parse request body as JSON into type T
/// - `Json<T>` as return: Serialize T to JSON response
async fn create_room(State(state): State<AppState>) -> Json<CreateRoomResponse> {
    let room_id = RoomId::generate();

    // Add room to state
    {
        let mut rooms = state.rooms.write().await;
        rooms.insert(room_id.clone(), Room::new(room_id.clone()));
    }

    info!("Created room: {}", room_id.as_str());

    Json(CreateRoomResponse {
        room_id: room_id.as_str().to_string(),
    })
}

/// Response for room info.
#[derive(Serialize)]
struct RoomInfoResponse {
    room_id: String,
    device_count: usize,
    devices: Vec<String>,
}

/// Gets information about a room.
///
/// ## Rust Concept: `Path` Extractor
///
/// `Path<String>` extracts the `:room_id` from the URL.
/// For multiple path params: `Path<(String, i32)>` or a custom struct.
async fn get_room_info(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<RoomInfoResponse>, axum::http::StatusCode> {
    let rooms = state.rooms.read().await;
    let room_id = RoomId(room_id);

    match rooms.get(&room_id) {
        Some(room) => Ok(Json(RoomInfoResponse {
            room_id: room.id.as_str().to_string(),
            device_count: room.devices.len(),
            devices: room.devices.values().cloned().collect(),
        })),
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

// ============================================================================
// WEBSOCKET HANDLER
// ============================================================================
/// Handles WebSocket connection upgrades.
///
/// ## Rust Concept: WebSocket Upgrade
///
/// WebSocket connections start as HTTP, then "upgrade" to WebSocket.
/// Axum's `WebSocketUpgrade` extractor handles this automatically.
///
/// The handler returns a response that includes upgrade headers,
/// and provides a callback that will be called with the WebSocket.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    // ## Rust Concept: Closures with `move`
    //
    // `move |socket|` creates a closure that takes OWNERSHIP of captured
    // variables (state, room_id). This is needed because the closure will
    // be called later, after this function returns.
    ws.on_upgrade(move |socket| handle_socket(socket, state, RoomId(room_id)))
}

/// Handles an individual WebSocket connection.
///
/// This is where the real work happens:
/// 1. Subscribe to room messages
/// 2. Forward incoming messages to the room
/// 3. Forward room messages to this client
async fn handle_socket(socket: WebSocket, state: AppState, room_id: RoomId) {
    // ## Rust Concept: Stream splitting
    //
    // A WebSocket is bidirectional - you can send AND receive.
    // `split()` gives us separate sender and receiver halves.
    // This lets us handle sending and receiving concurrently.
    let (mut sender, mut receiver) = socket.split();

    // Check if room exists and subscribe to it
    let tx = {
        let rooms = state.rooms.read().await;
        match rooms.get(&room_id) {
            Some(room) => room.tx.clone(),
            None => {
                warn!("Attempt to connect to non-existent room: {:?}", room_id);
                // Send error and close
                let _ = sender
                    .send(WsMessage::Text(
                        serde_json::to_string(&Message::Error {
                            code: clipsync_core::protocol::ErrorCode::Unknown,
                            message: "Room not found".to_string(),
                        })
                        .unwrap_or_default()
                        .into(),
                    ))
                    .await;
                return;
            }
        }
    };

    // Subscribe to room broadcasts
    let mut rx = tx.subscribe();

    // Generate a temporary ID for this connection
    let conn_id = Uuid::new_v4();
    debug!("New WebSocket connection {} in room {:?}", conn_id, room_id);

    // ## Rust Concept: `tokio::select!`
    //
    // `select!` waits for MULTIPLE async operations simultaneously.
    // It runs until ONE of them completes, then that branch is taken.
    // The others are cancelled.
    //
    // This is how we handle sending and receiving at the same time.
    loop {
        tokio::select! {
            // Branch 1: Receive a message from the room (broadcast from another device)
            Ok(msg) = rx.recv() => {
                // Serialize and send to this client
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        if sender.send(WsMessage::Text(json.into())).await.is_err() {
                            // Client disconnected
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to serialize message: {}", e);
                    }
                }
            }

            // Branch 2: Receive a message from this client
            Some(result) = receiver.next() => {
                match result {
                    Ok(WsMessage::Text(text)) => {
                        // Parse and broadcast to room
                        match serde_json::from_str::<Message>(&text) {
                            Ok(msg) => {
                                debug!("Received message type: {}", msg.message_type());
                                // Broadcast to all other devices in the room
                                if let Err(e) = tx.send(msg) {
                                    error!("Failed to broadcast message: {}", e);
                                }
                            }
                            Err(e) => {
                                warn!("Invalid message received: {}", e);
                            }
                        }
                    }
                    Ok(WsMessage::Close(_)) => {
                        debug!("Client {} requested close", conn_id);
                        break;
                    }
                    Ok(_) => {
                        // Ignore other message types (Binary, Ping, Pong)
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }

            // Branch 3: If both channels are exhausted, exit
            else => break,
        }
    }

    debug!("WebSocket connection {} closed", conn_id);
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_id_generation() {
        let id1 = RoomId::generate();
        let id2 = RoomId::generate();

        // Each ID should be unique
        assert_ne!(id1, id2);

        // IDs should be 6 characters
        assert_eq!(id1.as_str().len(), 6);

        // IDs should only contain valid characters
        for c in id1.as_str().chars() {
            assert!(c.is_ascii_alphanumeric());
        }
    }

    #[tokio::test]
    async fn test_app_state() {
        let state = AppState::new();

        // Initially no rooms
        assert!(state.rooms.read().await.is_empty());

        // Add a room
        let room_id = RoomId::generate();
        state
            .rooms
            .write()
            .await
            .insert(room_id.clone(), Room::new(room_id.clone()));

        // Now we have one room
        assert_eq!(state.rooms.read().await.len(), 1);
    }
}
