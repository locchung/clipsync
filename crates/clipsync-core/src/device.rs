//! # Device Identity
//!
//! This module handles device identification and pairing.
//!
//! Each device gets a unique ID when first run, stored locally.
//! Devices can be "paired" together to share clipboards.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// DEVICE ID
// ============================================================================
/// A unique identifier for a device.
///
/// ## Rust Concept: Newtype Pattern
///
/// `DeviceId(Uuid)` is a "newtype" - a single-field tuple struct.
/// It wraps an existing type to:
/// 1. Add type safety (can't accidentally mix DeviceId with other UUIDs)
/// 2. Implement custom behavior
/// 3. Hide the internal representation
///
/// ```rust
/// let device_id = DeviceId::new();
/// let user_id = UserId::new();  // Different type!
/// 
/// // These are incompatible even though both are UUIDs internally:
/// // fn process_device(d: DeviceId) { ... }
/// // process_device(user_id);  // ERROR: wrong type!
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(Uuid);

impl DeviceId {
    /// Creates a new random device ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a DeviceId from an existing UUID.
    ///
    /// ## Rust Concept: `From` Trait
    ///
    /// Instead of writing `DeviceId(some_uuid)` everywhere,
    /// we implement `From<Uuid>` to enable `.into()` conversion.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID.
    ///
    /// ## Rust Concept: Inner Value Access
    ///
    /// For newtypes, provide a way to get the inner value.
    /// Common patterns:
    /// - `.inner()` or `.as_inner()`: Returns reference
    /// - `.into_inner()`: Takes ownership and returns the value
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

/// ## Rust Concept: `Default` Trait
///
/// `Default` provides a default value for a type.
/// Many Rust functions/macros use `Default::default()` automatically.
///
/// For `DeviceId`, the default is a new random ID.
impl Default for DeviceId {
    fn default() -> Self {
        Self::new()
    }
}

/// ## Rust Concept: `Display` Trait
///
/// `Display` controls how a type is printed with `{}` format.
/// - `Debug` (derive): for debugging, uses `{:?}`
/// - `Display` (manual): for user-friendly output, uses `{}`
impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Just delegate to the UUID's display format
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// DEVICE INFO
// ============================================================================
/// Complete information about a device.
///
/// This is what gets shared with other paired devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Unique identifier
    pub id: DeviceId,
    
    /// Human-readable name (e.g., "John's MacBook", "My Phone")
    pub name: String,
    
    /// Type of device (for displaying appropriate icons)
    pub device_type: DeviceType,
    
    /// Platform/OS information
    pub platform: String,
}

/// Types of devices we support.
///
/// ## Rust Concept: Simple Enums
///
/// When an enum has no data in its variants, it's like C-style enums.
/// But we can still derive serde to serialize as strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
// ^ This attribute makes serde serialize as "desktop", "mobile", etc.
pub enum DeviceType {
    Desktop,
    Laptop,
    Mobile,
    Tablet,
    Unknown,
}

impl Device {
    /// Creates a new device with the given name.
    ///
    /// ## Rust Concept: Platform Detection
    ///
    /// The `cfg!()` macro checks compile-time configuration.
    /// - `target_os`: Operating system ("windows", "macos", "linux", etc.)
    /// - `target_arch`: CPU architecture ("x86_64", "aarch64", etc.)
    pub fn new(name: impl Into<String>) -> Self {
        let platform = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "linux") {
            "Linux"
        } else if cfg!(target_os = "android") {
            "Android"
        } else if cfg!(target_os = "ios") {
            "iOS"
        } else {
            "Unknown"
        };

        Self {
            id: DeviceId::new(),
            name: name.into(),
            device_type: DeviceType::Unknown, // Will be set based on platform
            platform: platform.to_string(),
        }
    }

    /// Sets the device type.
    ///
    /// ## Rust Concept: Builder Pattern
    ///
    /// Methods returning `Self` enable chaining:
    /// ```rust
    /// let device = Device::new("My Phone")
    ///     .with_type(DeviceType::Mobile);
    /// ```
    pub fn with_type(mut self, device_type: DeviceType) -> Self {
        self.device_type = device_type;
        self
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_id_uniqueness() {
        let id1 = DeviceId::new();
        let id2 = DeviceId::new();
        
        // Each call to new() should create a different ID
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_device_creation() {
        let device = Device::new("Test Device").with_type(DeviceType::Desktop);
        
        assert_eq!(device.name, "Test Device");
        assert_eq!(device.device_type, DeviceType::Desktop);
    }

    #[test]
    fn test_device_id_display() {
        let id = DeviceId::new();
        let display = format!("{}", id);
        
        // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (36 chars)
        assert_eq!(display.len(), 36);
        assert!(display.contains('-'));
    }
}
