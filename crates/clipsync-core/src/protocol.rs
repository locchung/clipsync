//! # Network Protocol
//!
//! This module defines the messages exchanged between devices.
//!
//! ## Rust Learning: Async/Await
//!
//! Rust supports asynchronous programming for non-blocking I/O.
//! Key concepts:
//!
//! - `async fn`: A function that can be paused and resumed
//! - `.await`: Pauses until the async operation completes
//! - `Future`: The type returned by async functions (represents ongoing work)
//!
//! ```rust
//! async fn fetch_data() -> Result<Data, Error> {
//!     let response = http_client.get(url).await?;  // Pauses here
//!     let data = response.json().await?;           // And here
//!     Ok(data)
//! }
//! ```
//!
//! Async code requires a RUNTIME (like Tokio) to actually execute.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::EncryptedData;
use crate::device::DeviceId;

// ============================================================================
// ERROR TYPES
// ============================================================================
/// Errors that can occur during message processing.
#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Failed to serialize message: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    #[error("Unknown message type: {0}")]
    UnknownMessageType(String),

    #[error("Invalid message format")]
    InvalidFormat,
}

// ============================================================================
// MESSAGE TYPES
// ============================================================================
/// All possible messages in the ClipSync protocol.
///
/// ## Protocol Design
///
/// Messages are grouped by purpose:
/// 1. **Discovery**: Finding devices on local network
/// 2. **Pairing**: Establishing secure connection
/// 3. **Sync**: Sharing clipboard content
/// 4. **Control**: Connection management
///
/// ## Rust Concept: Tagged Enums for Protocols
///
/// Rust enums are perfect for network protocols because:
/// - Each variant is a different message type
/// - Data is attached directly to the variant
/// - `match` ensures we handle all message types
/// - Serde can serialize to {"type": "MessageName", "data": {...}}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
// ^ This creates a "tagged" JSON format:
//   { "type": "Ping", "data": {...} }
// Instead of: { "Ping": {...} }
pub enum Message {
    // ===== Discovery =====
    /// Broadcast to find devices on local network.
    /// Sent via UDP multicast.
    Ping,

    /// Response to Ping, announcing our presence.
    Pong {
        device_id: DeviceId,
        device_name: String,
        /// Our local IP address for direct connection
        local_address: String,
        /// Port we're listening on
        port: u16,
    },

    // ===== Pairing =====
    /// Request to pair with another device.
    /// Includes our public key for key exchange.
    PairRequest {
        device_id: DeviceId,
        device_name: String,
        /// X25519 public key for Diffie-Hellman
        #[serde(with = "base64_bytes")]
        public_key: [u8; 32],
    },

    /// Accept a pairing request.
    /// Includes our public key to complete key exchange.
    PairAccept {
        device_id: DeviceId,
        device_name: String,
        #[serde(with = "base64_bytes")]
        public_key: [u8; 32],
    },

    /// Reject a pairing request.
    PairReject {
        device_id: DeviceId,
        reason: Option<String>,
    },

    // ===== Sync =====
    /// Clipboard content update (encrypted).
    ///
    /// The actual ClipboardItem is encrypted inside `encrypted_data`.
    /// This ensures the relay server cannot read clipboard contents.
    ClipboardUpdate {
        /// Unique ID for deduplication
        item_id: Uuid,
        /// The encrypted ClipboardItem
        encrypted_data: EncryptedData,
        /// Who sent this update
        from_device: DeviceId,
    },

    /// Acknowledge receipt of a clipboard update.
    /// Used for delivery confirmation and deduplication.
    Ack {
        item_id: Uuid,
    },

    /// Simple clipboard sync for local network P2P.
    /// No encryption needed on trusted LAN - just fast, direct sync.
    ClipboardSync {
        /// Who sent this
        from_device: DeviceId,
        /// Human-readable device name
        device_name: String,
        /// Text content (images/files not yet supported)
        content: String,
        /// When the content was copied (for last-write-wins conflict resolution)
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    // ===== Control =====
    /// Keep the connection alive.
    /// Sent periodically to detect disconnections.
    Heartbeat,

    /// Gracefully disconnect.
    Disconnect {
        device_id: DeviceId,
    },

    /// Error message from the other side.
    Error {
        code: ErrorCode,
        message: String,
    },
}

/// Error codes for the protocol.
///
/// ## Rust Concept: repr(u16)
///
/// By default, enum variants are stored as the smallest integer that fits.
/// `#[repr(u16)]` forces them to be stored as 16-bit unsigned integers.
/// This is useful for network protocols where you want explicit numeric codes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u16)]
pub enum ErrorCode {
    Unknown = 0,
    NotPaired = 1,
    DecryptionFailed = 2,
    InvalidMessage = 3,
    RateLimited = 4,
    StorageFull = 5,
}

impl Message {
    /// Serializes the message to JSON bytes.
    ///
    /// ## Rust Concept: `Result` Return Type
    ///
    /// We return `Result<Vec<u8>, MessageError>`:
    /// - `Ok(bytes)`: Serialization succeeded
    /// - `Err(error)`: Something went wrong
    ///
    /// The `?` operator uses `From` trait to convert serde_json::Error
    /// into our MessageError (see the `#[from]` attribute above).
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Deserializes a message from JSON bytes.
    ///
    /// ## Rust Concept: `impl Trait` in Return Position
    ///
    /// We could also write this as:
    /// `pub fn from_bytes(bytes: impl AsRef<[u8]>) -> ...`
    ///
    /// `impl AsRef<[u8]>` means "anything that can be viewed as a byte slice":
    /// - `&[u8]` (byte slice)
    /// - `Vec<u8>` (byte vector)
    /// - `String` (UTF-8 bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        Ok(serde_json::from_slice(bytes)?)
    }

    /// Returns a human-readable description of this message type.
    ///
    /// ## Rust Concept: `matches!` Macro
    ///
    /// `matches!` is a convenient way to check if a value matches a pattern
    /// without extracting the data. Returns `bool`.
    pub fn message_type(&self) -> &'static str {
        match self {
            Message::Ping => "ping",
            Message::Pong { .. } => "pong",
            Message::PairRequest { .. } => "pair_request",
            Message::PairAccept { .. } => "pair_accept",
            Message::PairReject { .. } => "pair_reject",
            Message::ClipboardUpdate { .. } => "clipboard_update",
            Message::Ack { .. } => "ack",
            Message::ClipboardSync { .. } => "clipboard_sync",
            Message::Heartbeat => "heartbeat",
            Message::Disconnect { .. } => "disconnect",
            Message::Error { .. } => "error",
        }
    }

    /// Checks if this message requires an established pairing.
    ///
    /// Discovery and pairing messages can be sent without being paired.
    /// Everything else requires an existing secure connection.
    pub fn requires_pairing(&self) -> bool {
        matches!(
            self,
            Message::ClipboardUpdate { .. } | Message::Ack { .. } | Message::Heartbeat
        )
    }
}

// ============================================================================
// SERIALIZATION HELPERS
// ============================================================================
/// Serde helper for base64-encoding [u8; 32] arrays (public keys).
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = STANDARD.decode(&s).map_err(serde::de::Error::custom)?;

        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("public key must be 32 bytes"))
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Test message serialization roundtrip.
    #[test]
    fn test_message_serialization() {
        // Create a Ping message
        let msg = Message::Ping;

        // Serialize to bytes
        let bytes = msg.to_bytes().expect("serialization should work");

        // Deserialize back
        let decoded = Message::from_bytes(&bytes).expect("deserialization should work");

        // Check it matches
        assert!(matches!(decoded, Message::Ping));
    }

    /// Test Pong message with data.
    #[test]
    fn test_pong_message() {
        let msg = Message::Pong {
            device_id: DeviceId::new(),
            device_name: "Test Device".to_string(),
            local_address: "192.168.1.100".to_string(),
            port: 8765,
        };

        let bytes = msg.to_bytes().unwrap();
        let json = String::from_utf8(bytes.clone()).unwrap();

        // Check the JSON structure
        assert!(json.contains("type"));
        assert!(json.contains("Pong"));
        assert!(json.contains("Test Device"));

        // Deserialize back
        let decoded = Message::from_bytes(&bytes).unwrap();
        if let Message::Pong { device_name, port, .. } = decoded {
            assert_eq!(device_name, "Test Device");
            assert_eq!(port, 8765);
        } else {
            panic!("Expected Pong message");
        }
    }

    /// Test that pairing messages don't require pairing.
    #[test]
    fn test_requires_pairing() {
        assert!(!Message::Ping.requires_pairing());
        assert!(!Message::Pong {
            device_id: DeviceId::new(),
            device_name: "Test".into(),
            local_address: "127.0.0.1".into(),
            port: 8080,
        }
        .requires_pairing());

        assert!(Message::Heartbeat.requires_pairing());
        assert!(Message::Ack {
            item_id: Uuid::new_v4()
        }
        .requires_pairing());
    }
}
