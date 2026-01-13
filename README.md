# ClipSync

**Cross-Platform Clipboard Sharing** — Seamlessly sync your clipboard between devices on your local network or via cloud relay.

## ✨ Features

- 🔒 **End-to-End Encryption** — AES-256-GCM encryption with X25519 key exchange
- 🌐 **Local Network Discovery** — Automatic device discovery via mDNS
- ☁️ **Cloud Relay** — Sync across networks when devices aren't on the same LAN
- 📋 **Rich Content Support** — Text, images, and file references
- 🖥️ **Cross-Platform** — Desktop support via Tauri (Linux, macOS, Windows)

## 📦 Project Structure

```
clipsync/
├── crates/
│   ├── clipsync-core/      # Shared library (data types, crypto, protocol)
│   ├── clipsync-cli/       # CLI tool for local network sync
│   ├── clipsync-server/    # Cloud relay server (WebSocket-based)
│   └── clipsync-desktop/   # Desktop application (Tauri)
├── install.sh              # One-line install script
├── Cargo.toml              # Workspace configuration
└── Cargo.lock
```

### Crates

| Crate | Description |
|-------|-------------|
| `clipsync-core` | Core library with clipboard types, encryption, protocol definitions, and device discovery |
| `clipsync-cli` | **CLI tool** for peer-to-peer clipboard sync on local networks |
| `clipsync-server` | WebSocket relay server for cross-network clipboard sync |
| `clipsync-desktop` | Desktop clipboard monitor demo |

## 🚀 Quick Start

### Install (One Command)

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/locchung/clipsync/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
iwr -useb https://raw.githubusercontent.com/locchung/clipsync/main/install.ps1 | iex
```

**With Rust installed (any OS):**
```bash
cargo install --git https://github.com/locchung/clipsync --bin clipsync
```

### Usage

#### Local Network Mode (Default)

Run `clipsync` on each device you want to sync:

```bash
# Laptop A
clipsync

# Laptop B (on the same network)
clipsync
```

That's it! Copy text on one device, and it automatically appears on the other.

```
🔗 ClipSync - Clipboard Sharing
================================
Device: laptop-a (fd26de72)
Mode: Local Network
📡 Listening on port 43210
🔍 Discovering peers on local network...

🔎 Found: laptop-b (a1b2c3d4)
✅ Connected: laptop-b (outgoing)
📤 Synced: "Hello from Laptop A!" -> 1 peer(s)
📥 laptop-b -> "Reply from Laptop B!"
```

#### Cloud Relay Mode (Different Networks)

For syncing across different networks (e.g., home and office), use relay mode:

```bash
# Device A: Create a room
clipsync --relay --create-room

# Output:
# 🔗 ClipSync - Clipboard Sharing
# ================================
# Device: laptop-a (fd26de72)
# Mode: Cloud Relay
# 📋 Created room: ABC123
# Share this code with other devices to sync clipboards!

# Device B: Join the room using the code
clipsync --relay --join ABC123
```

**CLI Options:**
| Flag | Description |
|------|-------------|
| `--relay` | Enable cloud relay mode |
| `--server <URL>` | Custom relay server URL (default: `ws://109.123.237.29:3002`) |
| `--create-room` | Create a new room (returns a 6-character code) |
| `--join <CODE>` | Join an existing room by code |

### Prerequisites

**Local Mode:**
- Two or more devices on the **same local network**
- That's it! No accounts, no server, no configuration

**Cloud Mode:**
- A relay server (you can self-host or use the default server)
- Share the room code between devices

---

### Run the Server

```bash
cargo run --bin clipsync-server
```

The relay server starts on `http://0.0.0.0:3002` with the following endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/rooms` | POST | Create a new room (returns join code) |
| `/rooms/:room_id` | GET | Get room info |
| `/rooms/:room_id/ws` | GET | WebSocket connection for real-time sync |

### Run the Desktop Demo

```bash
cargo run --bin clipsync-desktop
```

This starts a clipboard monitor that detects and logs clipboard changes:

```
INFO clipsync_desktop: ClipSync Desktop - Clipboard Monitor Demo
INFO clipsync_desktop: Device ID: fd26de72-6d8b-4eb0-88a1-7c9fe7485ca2
INFO clipsync_desktop: Platform: Linux
INFO clipsync_desktop: Monitoring clipboard for changes...
INFO clipsync_desktop: 📋 Clipboard changed!
INFO clipsync_desktop:   Content: Hello, World!
INFO clipsync_desktop:   Size: 13 bytes
```

## 🔧 Architecture

### Core Library (`clipsync-core`)

- **`clipboard`** — Data types for clipboard content (text, images, files)
- **`crypto`** — AES-256-GCM encryption and X25519 key exchange
- **`protocol`** — Network message format (JSON-serializable)
- **`discovery`** — mDNS-based local network device discovery
- **`device`** — Device identity and pairing

### Relay Server (`clipsync-server`)

The server uses **rooms** to group paired devices:

1. Device A creates a room → gets a 6-character join code
2. Device B joins using the code
3. Both devices connect via WebSocket
4. Clipboard updates are broadcast to all devices in the room

### Protocol Messages

```rust
enum Message {
    Hello { device: Device },
    ClipboardUpdate { item: ClipboardItem },
    ClipboardRequest { since: Option<DateTime> },
    Ping,
    Pong,
    Error { code: ErrorCode, message: String },
}
```

## 🔐 Security

- **Key Exchange**: X25519 Elliptic Curve Diffie-Hellman
- **Encryption**: AES-256-GCM authenticated encryption
- **No Plaintext**: Clipboard content is encrypted before leaving the device
- **Room Codes**: 6-character alphanumeric codes (avoids confusing characters like 0/O, 1/I/l)

## 📝 License

MIT

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
