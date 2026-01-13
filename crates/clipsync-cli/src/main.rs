//! # ClipSync CLI
//!
//! A command-line tool for clipboard sharing between devices on the same network.
//!
//! ## How It Works
//!
//! 1. Announces via UDP broadcast + mDNS (dual discovery for reliability)
//! 2. Discovers other ClipSync instances automatically
//! 3. Connects via TCP for clipboard sync
//! 4. Monitors local clipboard and syncs changes to peers
//!
//! ## Usage
//!
//! Just run `clipsync` on each device. They'll find each other automatically!

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use arboard::Clipboard;
use chrono::Utc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use clipsync_core::device::{Device, DeviceType};
use clipsync_core::discovery::{DiscoveryEvent, DiscoveryService, DEFAULT_PORT};
use clipsync_core::protocol::Message;
use clipsync_core::DeviceId;
use uuid::Uuid;

const BROADCAST_PORT: u16 = 43211;

// ============================================================================
// PEER CONNECTION
// ============================================================================
/// Represents a connected peer device.
#[derive(Debug, Clone)]
#[allow(dead_code)] // addr kept for future reconnection logic
struct Peer {
    device_id: DeviceId,
    device_name: String,
    addr: SocketAddr,
}

// ============================================================================
// APP STATE
// ============================================================================
/// Shared state for the CLI application.
struct AppState {
    /// Our device identity
    device: Device,
    /// Connected peers (device_id -> peer info)
    peers: RwLock<HashMap<DeviceId, Peer>>,
    /// Channel to broadcast clipboard updates to all connection handlers
    clipboard_tx: broadcast::Sender<Message>,
    /// Last clipboard content we set (to avoid echo)
    last_set_content: RwLock<Option<String>>,
    /// Last received clipboard timestamp (for conflict resolution)
    last_received_timestamp: RwLock<Option<chrono::DateTime<Utc>>>,
}

impl AppState {
    fn new(device: Device) -> Arc<Self> {
        let (clipboard_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            device,
            peers: RwLock::new(HashMap::new()),
            clipboard_tx,
            last_set_content: RwLock::new(None),
            last_received_timestamp: RwLock::new(None),
        })
    }
}

// ============================================================================
// MAIN
// ============================================================================
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("clipsync=info".parse()?)
                .add_directive("clipsync_cli=info".parse()?)
                .add_directive("clipsync_core=info".parse()?),
        )
        .init();

    // Create our device identity
    let device = Device::new(&get_device_name()).with_type(DeviceType::Desktop);

    info!("🔗 ClipSync - Clipboard Sharing");
    info!("================================");
    info!("Device: {} ({})", device.name, &device.id.to_string()[..8]);
    info!("Press Ctrl+C to exit\n");

    // Create shared state
    let state = AppState::new(device.clone());

    // Start TCP server
    let listener = TcpListener::bind(format!("0.0.0.0:{}", DEFAULT_PORT)).await?;
    info!("📡 Listening on port {}", DEFAULT_PORT);

    // Spawn connection acceptor
    let state_clone = Arc::clone(&state);
    tokio::spawn(accept_connections(listener, state_clone));

    // Spawn clipboard monitor
    let state_clone = Arc::clone(&state);
    tokio::spawn(monitor_clipboard(state_clone));

    // Start UDP broadcast discovery (simple and reliable)
    let state_clone = Arc::clone(&state);
    let device_clone = device.clone();
    tokio::spawn(udp_broadcast_sender(device_clone));
    
    let state_clone2 = Arc::clone(&state);
    tokio::spawn(udp_broadcast_receiver(state_clone2));

    // Also try mDNS (may work on some networks)
    if let Ok(mut discovery) = DiscoveryService::new() {
        let _ = discovery.announce(&device, DEFAULT_PORT);
        if let Ok(discovery_rx) = discovery.browse() {
            let state_clone = Arc::clone(&state);
            tokio::spawn(handle_discovery(discovery_rx, state_clone));
        }
        std::mem::forget(discovery);
    }

    info!("🔍 Discovering peers on local network...\n");

    // Wait forever (until Ctrl+C)
    tokio::signal::ctrl_c().await?;
    info!("\n👋 Shutting down...");

    Ok(())
}

// ============================================================================
// UDP BROADCAST DISCOVERY (Simple fallback when mDNS is blocked)
// ============================================================================
/// Sends UDP broadcast every 2 seconds to announce our presence
async fn udp_broadcast_sender(device: Device) {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            debug!("Failed to create UDP socket: {}", e);
            return;
        }
    };
    
    if let Err(e) = socket.set_broadcast(true) {
        debug!("Failed to enable broadcast: {}", e);
        return;
    }

    let announce = format!("CLIPSYNC:{}:{}:{}", device.id, device.name, DEFAULT_PORT);
    let broadcast_addr: SocketAddr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
        BROADCAST_PORT
    );

    loop {
        let _ = socket.send_to(announce.as_bytes(), broadcast_addr);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Listens for UDP broadcasts from other ClipSync instances
async fn udp_broadcast_receiver(state: Arc<AppState>) {
    let socket = match UdpSocket::bind(format!("0.0.0.0:{}", BROADCAST_PORT)) {
        Ok(s) => s,
        Err(e) => {
            debug!("Failed to bind UDP listener: {}", e);
            return;
        }
    };
    
    let _ = socket.set_nonblocking(true);
    let mut buf = [0u8; 256];

    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        while let Ok((len, src_addr)) = socket.recv_from(&mut buf) {
            let msg = String::from_utf8_lossy(&buf[..len]);
            
            if let Some(parts) = parse_broadcast(&msg) {
                let (device_id_str, device_name, _port) = parts;
                
                // Parse device ID
                if let Ok(uuid) = uuid::Uuid::parse_str(&device_id_str) {
                    let device_id = DeviceId::from_uuid(uuid);
                    
                    // Skip ourselves
                    if device_id == state.device.id {
                        continue;
                    }
                    
                    // Skip if already connected
                    {
                        let peers = state.peers.read().await;
                        if peers.contains_key(&device_id) {
                            continue;
                        }
                    }
                    
                    info!("🔎 Found: {} ({})", device_name, &device_id_str[..8]);
                    
                    // Connect to peer
                    let addr = SocketAddr::new(src_addr.ip(), DEFAULT_PORT);
                    let state_clone = Arc::clone(&state);
                    tokio::spawn(async move {
                        if let Err(e) = connect_to_peer_by_addr(addr, state_clone).await {
                            debug!("Failed to connect: {}", e);
                        }
                    });
                }
            }
        }
    }
}

fn parse_broadcast(msg: &str) -> Option<(String, String, u16)> {
    let parts: Vec<&str> = msg.split(':').collect();
    if parts.len() >= 4 && parts[0] == "CLIPSYNC" {
        let port = parts[3].parse().unwrap_or(DEFAULT_PORT);
        return Some((parts[1].to_string(), parts[2].to_string(), port));
    }
    None
}

async fn connect_to_peer_by_addr(addr: SocketAddr, state: Arc<AppState>) -> Result<()> {
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(addr),
    ).await??;
    
    handle_connection(stream, addr, state, false).await
}

// ============================================================================
// DEVICE NAME
// ============================================================================
fn get_device_name() -> String {
    hostname::get()
        .map(|h: std::ffi::OsString| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Unknown".to_string())
}

// ============================================================================
// DISCOVERY HANDLER
// ============================================================================
async fn handle_discovery(
    mut rx: broadcast::Receiver<DiscoveryEvent>,
    state: Arc<AppState>,
) {
    while let Ok(event) = rx.recv().await {
        match event {
            DiscoveryEvent::DeviceFound {
                device_id,
                device_name,
                addresses,
                port,
            } => {
                // Skip ourselves
                if device_id == state.device.id {
                    continue;
                }

                // Check if we're already connected
                {
                    let peers = state.peers.read().await;
                    if peers.contains_key(&device_id) {
                        continue;
                    }
                }

                info!("🔎 Found: {} ({})", device_name, &device_id.to_string()[..8]);

                // Try to connect to the peer
                if let Some(addr) = addresses.first() {
                    let socket_addr = SocketAddr::new(*addr, port);
                    let state_clone = Arc::clone(&state);
                    tokio::spawn(async move {
                        if let Err(e) = connect_to_peer(socket_addr, device_id.clone(), device_name.clone(), state_clone).await {
                            debug!("Failed to connect to {}: {}", device_name, e);
                        }
                    });
                }
            }
            DiscoveryEvent::DeviceLost { device_id } => {
                let mut peers = state.peers.write().await;
                if let Some(peer) = peers.remove(&device_id) {
                    info!("👋 Lost: {}", peer.device_name);
                }
            }
        }
    }
}

// ============================================================================
// TCP SERVER
// ============================================================================
async fn accept_connections(listener: TcpListener, state: Arc<AppState>) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                debug!("Incoming connection from {}", addr);
                let state_clone = Arc::clone(&state);
                tokio::spawn(handle_connection(stream, addr, state_clone, true));
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

// ============================================================================
// CONNECT TO PEER
// ============================================================================
async fn connect_to_peer(
    addr: SocketAddr,
    device_id: DeviceId,
    device_name: String,
    state: Arc<AppState>,
) -> Result<()> {
    // Avoid connecting if already connected
    {
        let peers = state.peers.read().await;
        if peers.contains_key(&device_id) {
            return Ok(());
        }
    }

    // Connect with timeout
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(addr),
    )
    .await??;

    debug!("Connected to {} at {}", device_name, addr);

    // Handle the connection
    handle_connection(stream, addr, state, false).await
}

// ============================================================================
// CONNECTION HANDLER
// ============================================================================
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<AppState>,
    is_server: bool,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Send hello message
    let hello = Message::Pong {
        device_id: state.device.id.clone(),
        device_name: state.device.name.clone(),
        local_address: addr.ip().to_string(),
        port: DEFAULT_PORT,
    };
    let msg = serde_json::to_string(&hello)? + "\n";
    writer.write_all(msg.as_bytes()).await?;

    // Read peer's hello
    line.clear();
    reader.read_line(&mut line).await?;
    
    let peer_info = match serde_json::from_str::<Message>(&line) {
        Ok(Message::Pong { device_id, device_name, .. }) => {
            // Skip if this is ourselves
            if device_id == state.device.id {
                return Ok(());
            }

            // Check if already connected
            {
                let peers = state.peers.read().await;
                if peers.contains_key(&device_id) {
                    return Ok(());
                }
            }

            Peer {
                device_id: device_id.clone(),
                device_name: device_name.clone(),
                addr,
            }
        }
        _ => {
            warn!("Invalid hello from {}", addr);
            return Ok(());
        }
    };

    // Add to peers
    let peer_id = peer_info.device_id.clone();
    let peer_name = peer_info.device_name.clone();
    {
        let mut peers = state.peers.write().await;
        peers.insert(peer_id.clone(), peer_info);
    }

    info!(
        "✅ Connected: {} ({})",
        peer_name,
        if is_server { "incoming" } else { "outgoing" }
    );

    // Subscribe to clipboard updates
    let mut clipboard_rx = state.clipboard_tx.subscribe();

    // Handle bidirectional communication
    loop {
        tokio::select! {
            // Send clipboard updates to this peer
            Ok(msg) = clipboard_rx.recv() => {
                let json = serde_json::to_string(&msg)? + "\n";
                if writer.write_all(json.as_bytes()).await.is_err() {
                    break;
                }
            }

            // Receive messages from this peer
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break, // Connection closed
                    Ok(_) => {
                        if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                            handle_message(msg, &state).await;
                        }
                        line.clear();
                    }
                    Err(_) => break,
                }
            }
        }
    }

    // Remove from peers
    {
        let mut peers = state.peers.write().await;
        peers.remove(&peer_id);
    }
    info!("❌ Disconnected: {}", peer_name);

    Ok(())
}

// ============================================================================
// MESSAGE HANDLER
// ============================================================================
async fn handle_message(msg: Message, state: &AppState) {
    match msg {
        Message::ClipboardSync {
            from_device,
            device_name,
            content,
            timestamp,
        } => {
            // Skip if from ourselves
            if from_device == state.device.id {
                return;
            }

            // Check timestamp for conflict resolution (last-write-wins)
            {
                let last_ts = state.last_received_timestamp.read().await;
                if let Some(last) = *last_ts {
                    if timestamp <= last {
                        debug!("Ignoring older clipboard update");
                        return;
                    }
                }
            }

            // Update last received timestamp
            {
                let mut last_ts = state.last_received_timestamp.write().await;
                *last_ts = Some(timestamp);
            }

            // Set local clipboard
            match Clipboard::new() {
                Ok(mut clipboard) => {
                    // Remember what we're setting (to avoid echo)
                    {
                        let mut last = state.last_set_content.write().await;
                        *last = Some(content.clone());
                    }

                    if let Err(e) = clipboard.set_text(&content) {
                        error!("Failed to set clipboard: {}", e);
                    } else {
                        let preview = if content.len() > 50 {
                            format!("{}...", &content[..50])
                        } else {
                            content.clone()
                        };
                        info!("📥 {} -> \"{}\"", device_name, preview);
                    }
                }
                Err(e) => {
                    error!("Failed to access clipboard: {}", e);
                }
            }
        }
        _ => {
            debug!("Received message: {}", msg.message_type());
        }
    }
}

// ============================================================================
// CLIPBOARD MONITOR
// ============================================================================
async fn monitor_clipboard(state: Arc<AppState>) {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to access clipboard: {}", e);
            return;
        }
    };

    let mut last_content: Option<String> = None;

    loop {
        // Poll clipboard every 200ms
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Get current clipboard text
        let current = match clipboard.get_text() {
            Ok(text) => Some(text),
            Err(_) => None,
        };

        // Check if changed
        if current != last_content {
            if let Some(ref text) = current {
                // Check if this is content we just set (avoid echo)
                {
                    let last_set = state.last_set_content.read().await;
                    if let Some(ref last) = *last_set {
                        if last == text {
                            last_content = current;
                            continue;
                        }
                    }
                }

                // Clear the last set content
                {
                    let mut last_set = state.last_set_content.write().await;
                    *last_set = None;
                }

                // Check if we have any peers
                let peer_count = state.peers.read().await.len();
                if peer_count > 0 {
                    // Send to all peers
                    let msg = Message::ClipboardSync {
                        from_device: state.device.id.clone(),
                        device_name: state.device.name.clone(),
                        content: text.clone(),
                        timestamp: Utc::now(),
                    };

                    let _ = state.clipboard_tx.send(msg);

                    let preview = if text.len() > 50 {
                        format!("{}...", &text[..50])
                    } else {
                        text.clone()
                    };
                    info!("📤 Synced: \"{}\" -> {} peer(s)", preview, peer_count);
                }
            }

            last_content = current;
        }
    }
}
