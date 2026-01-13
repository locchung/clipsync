//! # Clipboard Data Types
//!
//! This module defines the types of content that can be shared:
//! - Text (strings)
//! - Images (raw bytes with MIME type)
//! - Files (with filename and content)
//!
//! ## Rust Learning: Ownership & Borrowing (FUNDAMENTAL!)
//!
//! Rust's killer feature is OWNERSHIP. Every value has exactly ONE owner.
//! When the owner goes out of scope, the value is dropped (freed).
//!
//! ```rust
//! let s1 = String::from("hello");
//! let s2 = s1;  // s1 is MOVED to s2, s1 is no longer valid!
//! // println!("{}", s1);  // ERROR: s1 was moved
//! ```
//!
//! To avoid moving, we BORROW with references:
//! - `&T`: Immutable borrow (read-only, can have many)
//! - `&mut T`: Mutable borrow (read-write, can only have ONE)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::device::DeviceId;

// ============================================================================
// CLIPBOARD CONTENT
// ============================================================================
/// Represents the actual content that can be in a clipboard.
///
/// ## Rust Concept: Enums with Data
///
/// Unlike C/Java enums that are just numbers, Rust enums can hold DATA.
/// Each variant is like a mini-struct with its own fields.
///
/// ```rust
/// let content = ClipboardContent::Text("Hello".to_string());
///
/// // Pattern matching to extract the data:
/// match content {
///     ClipboardContent::Text(s) => println!("Got text: {}", s),
///     ClipboardContent::Image { data, mime } => println!("Got image"),
///     _ => println!("Something else"),
/// }
/// ```
///
/// ## Rust Concept: Derive Macros
///
/// `#[derive(...)]` auto-generates trait implementations:
/// - `Debug`: Enables `println!("{:?}", value)` for debugging
/// - `Clone`: Enables `.clone()` to create a deep copy
/// - `PartialEq`: Enables `==` and `!=` comparisons
/// - `Serialize/Deserialize`: Enables JSON conversion (from serde)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipboardContent {
    /// Plain text content
    ///
    /// ## Rust Concept: String vs &str
    ///
    /// - `String`: Owned, heap-allocated, growable text. You own the memory.
    /// - `&str`: Borrowed reference to text. You're just looking at someone else's String.
    ///
    /// Use `String` when you need to own/store the data (like here in a struct).
    /// Use `&str` for function parameters when you just need to read the text.
    Text(String),

    /// Image data with MIME type (e.g., "image/png")
    ///
    /// ## Rust Concept: Struct-like Enum Variants
    ///
    /// Enum variants can have named fields like a struct.
    /// This makes the code more readable than tuple variants.
    Image {
        /// Raw image bytes
        ///
        /// ## Rust Concept: Vec<T>
        ///
        /// `Vec<u8>` is a growable array (vector) of bytes.
        /// - `Vec` = Vector, like ArrayList in Java or list in Python
        /// - `u8` = unsigned 8-bit integer (0-255), represents a byte
        ///
        /// Other number types:
        /// - `i8, i16, i32, i64`: Signed integers (can be negative)
        /// - `u8, u16, u32, u64`: Unsigned integers (positive only)
        /// - `f32, f64`: Floating point numbers
        data: Vec<u8>,
        
        /// MIME type like "image/png" or "image/jpeg"
        mime: String,
    },

    /// A single file
    File {
        /// Original filename
        name: String,
        /// File content as bytes
        data: Vec<u8>,
        /// MIME type like "application/pdf"
        mime: String,
    },

    /// Multiple files
    ///
    /// ## Rust Concept: Nested Types
    ///
    /// `Vec<FileEntry>` is a vector containing `FileEntry` structs.
    /// Rust allows unlimited nesting of generic types.
    Files(Vec<FileEntry>),
}

/// Represents a file in a multi-file clipboard operation.
///
/// ## Rust Concept: Structs
///
/// Structs are like classes without methods (those come from `impl` blocks).
/// All fields are PRIVATE by default. Add `pub` to make them public.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub mime: String,
}

impl ClipboardContent {
    /// Returns the size of the content in bytes.
    ///
    /// ## Rust Concept: `impl` Blocks
    ///
    /// Methods are defined in `impl` blocks, separate from the struct/enum.
    /// This lets you split methods across multiple files if needed.
    ///
    /// ## Rust Concept: `&self`
    ///
    /// - `&self`: Method borrows self immutably (read-only)
    /// - `&mut self`: Method borrows self mutably (can modify)
    /// - `self`: Method TAKES OWNERSHIP (consumes the value)
    ///
    /// ## Rust Concept: `match` Expression
    ///
    /// `match` is like switch-case but WAY more powerful:
    /// - Must handle ALL possible cases (exhaustive)
    /// - Can destructure values and bind variables
    /// - Returns a value (it's an expression!)
    pub fn size(&self) -> usize {
        match self {
            // For Text, just get the byte length of the string
            ClipboardContent::Text(s) => s.len(),
            
            // Destructure the Image variant to get `data`
            ClipboardContent::Image { data, .. } => data.len(),
            
            // `..` means "ignore the other fields"
            ClipboardContent::File { data, .. } => data.len(),
            
            // Sum up all file sizes using iterator methods
            ClipboardContent::Files(files) => {
                // ## Rust Concept: Iterators
                //
                // `.iter()` creates an iterator over the vector
                // `.map()` transforms each element
                // `.sum()` adds them all up
                //
                // This is like: files.map(f => f.data.length).reduce((a,b) => a+b, 0)
                files.iter().map(|f| f.data.len()).sum()
            }
        }
    }

    /// Returns a human-readable description of the content type.
    ///
    /// ## Rust Concept: `&'static str`
    ///
    /// `'static` is a LIFETIME. It means "this reference lives forever".
    /// String literals like "text" are baked into the program binary,
    /// so they're always valid - hence `'static` lifetime.
    pub fn content_type(&self) -> &'static str {
        match self {
            ClipboardContent::Text(_) => "text",
            ClipboardContent::Image { .. } => "image",
            ClipboardContent::File { .. } => "file",
            ClipboardContent::Files(_) => "files",
        }
    }
}

// ============================================================================
// CLIPBOARD ITEM
// ============================================================================
/// A complete clipboard item with metadata.
///
/// This wraps `ClipboardContent` with additional info like:
/// - Unique ID (for deduplication)
/// - Timestamp (when it was copied)
/// - Source device (who copied it)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    /// Unique identifier for this clipboard item
    ///
    /// ## Why UUIDs?
    ///
    /// UUIDs (Universally Unique IDs) are 128-bit random IDs.
    /// They're practically guaranteed to be unique across all devices.
    /// This helps us deduplicate (avoid syncing the same item twice).
    pub id: Uuid,
    
    /// The actual content
    pub content: ClipboardContent,
    
    /// When the content was copied
    ///
    /// ## Rust Concept: Generic Types
    ///
    /// `DateTime<Utc>` is a generic type. `DateTime` works with different
    /// time zones, and `Utc` specifies we're using UTC timezone.
    pub timestamp: DateTime<Utc>,
    
    /// Which device this came from
    pub source_device: DeviceId,
}

impl ClipboardItem {
    /// Creates a new clipboard item with a random ID and current timestamp.
    ///
    /// ## Rust Concept: Associated Functions vs Methods
    ///
    /// - `fn new(...)` - Associated function (no `self`), called as `Type::new()`
    /// - `fn size(&self)` - Method (has `self`), called as `instance.size()`
    ///
    /// Associated functions are like static methods in other languages.
    /// `new()` is the conventional name for constructors in Rust.
    pub fn new(content: ClipboardContent, source_device: DeviceId) -> Self {
        // ## Rust Concept: `Self`
        //
        // `Self` refers to the type we're implementing (`ClipboardItem`).
        // It's shorthand so we don't have to repeat the type name.
        Self {
            id: Uuid::new_v4(),        // Generate random UUID
            content,                    // Shorthand: `content: content,`
            timestamp: Utc::now(),      // Current UTC time
            source_device,              // Shorthand for `source_device: source_device`
        }
    }

    /// Creates a text clipboard item.
    ///
    /// ## Rust Concept: `impl Into<T>`
    ///
    /// `impl Into<String>` means "any type that can be converted to String".
    /// This accepts:
    /// - `String` (no conversion needed)
    /// - `&str` (converted via `.into()`)
    /// - Any other type implementing `Into<String>`
    ///
    /// The `.into()` call at the end does the conversion.
    pub fn text(text: impl Into<String>, source_device: DeviceId) -> Self {
        Self::new(ClipboardContent::Text(text.into()), source_device)
    }

    /// Creates an image clipboard item.
    pub fn image(data: Vec<u8>, mime: impl Into<String>, source_device: DeviceId) -> Self {
        Self::new(
            ClipboardContent::Image {
                data,
                mime: mime.into(),
            },
            source_device,
        )
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Test creating and measuring text content.
    #[test]
    fn test_text_content_size() {
        let content = ClipboardContent::Text("Hello, World!".to_string());
        
        // "Hello, World!" is 13 characters
        assert_eq!(content.size(), 13);
        assert_eq!(content.content_type(), "text");
    }

    /// Test creating a clipboard item.
    #[test]
    fn test_clipboard_item_creation() {
        let device_id = DeviceId::new();
        let item = ClipboardItem::text("Test content", device_id.clone());
        
        // Check that the ID was generated
        assert!(!item.id.is_nil());
        
        // Check the content
        // ## Rust Concept: `matches!` Macro
        //
        // `matches!` checks if a value matches a pattern.
        // Returns `true` or `false`.
        assert!(matches!(item.content, ClipboardContent::Text(_)));
        
        // Check the device
        assert_eq!(item.source_device, device_id);
    }
}
