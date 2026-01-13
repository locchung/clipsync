//! # ClipSync Core Library
//! 
//! This is the SHARED LIBRARY containing:
//! - Data types for clipboard content
//! - Encryption/decryption
//! - Network protocol
//! - Device discovery
//!
//! ## Rust Concept: Crate Root (`lib.rs`)
//! 
//! In Rust, `lib.rs` is the "root" of a library crate. It's like the entry point
//! that defines what's PUBLIC (accessible from outside) vs PRIVATE.
//!
//! Everything starts here, and we use `mod` to include other files.

// ============================================================================
// MODULE DECLARATIONS
// ============================================================================
// `mod` keyword declares a module. Rust will look for:
//   1. A file named `clipboard.rs` in the same directory, OR
//   2. A folder named `clipboard/` with a `mod.rs` inside
//
// `pub mod` makes the module PUBLIC - other crates can access it
// Without `pub`, it would be private to this crate only

pub mod clipboard;  // clipboard.rs - data types for clipboard content
pub mod crypto;     // crypto.rs - encryption/decryption
pub mod protocol;   // protocol.rs - network message format
pub mod discovery;  // discovery.rs - find devices on local network
pub mod device;     // device.rs - device identity and pairing

// ============================================================================
// RE-EXPORTS
// ============================================================================
// `pub use` re-exports items, making them accessible directly from the crate root.
// Instead of: use clipsync_core::clipboard::ClipboardContent;
// Users can: use clipsync_core::ClipboardContent;

pub use clipboard::{ClipboardContent, ClipboardItem};
pub use crypto::{CryptoError, KeyPair, SharedSecret};
pub use device::{Device, DeviceId};
pub use protocol::{Message, MessageError};

// ============================================================================
// COMMON TYPES
// ============================================================================
/// Result type alias for this crate.
/// 
/// ## Rust Concept: Type Aliases
/// 
/// `type Foo = Bar` creates an alias. It's the same type, just a shorter name.
/// Here, `Result<T>` is shorthand for `std::result::Result<T, Error>`.
/// 
/// ## Rust Concept: Generics
/// 
/// `<T>` is a "generic" - a placeholder for any type.
/// `Result<T>` means "Result where the success value can be any type T".
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for this crate.
/// 
/// ## Rust Concept: Enums
/// 
/// Rust enums are POWERFUL - each variant can hold different data.
/// Unlike C enums (just numbers), Rust enums are "tagged unions".
/// 
/// ## Rust Concept: `#[derive(...)]`
/// 
/// The `derive` attribute auto-generates code. Here:
/// - `Debug`: Lets us print the error with {:?} for debugging
/// - `thiserror::Error`: Generates `std::error::Error` implementation
/// 
/// ## Rust Concept: `#[error("...")]`
/// 
/// This attribute (from thiserror) defines the error message.
/// `{0}` refers to the first field in the variant.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Errors from encryption/decryption operations
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),
    
    /// Errors from message serialization
    #[error("Protocol error: {0}")]
    Protocol(#[from] MessageError),
    
    /// Network-related errors
    #[error("Network error: {0}")]
    Network(String),
    
    /// Clipboard access errors
    #[error("Clipboard error: {0}")]
    Clipboard(String),
    
    /// Generic I/O errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// TESTS
// ============================================================================
/// ## Rust Concept: Test Modules
/// 
/// `#[cfg(test)]` means "only compile this when running tests".
/// Tests live alongside the code they test, which is convenient!
/// 
/// Run tests with: `cargo test`
#[cfg(test)]
mod tests {
    // `use super::*` imports everything from the parent module
    use super::*;
    
    /// ## Rust Concept: Test Functions
    /// 
    /// `#[test]` marks this function as a test.
    /// Tests pass if they don't panic (crash).
    #[test]
    fn test_error_display() {
        let err = Error::Network("connection refused".to_string());
        // `assert!` panics if the condition is false
        assert!(err.to_string().contains("connection refused"));
    }
}
