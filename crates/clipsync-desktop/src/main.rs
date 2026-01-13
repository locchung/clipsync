//! # ClipSync Desktop Application (Placeholder)
//!
//! This will become the full Tauri desktop app.
//! For now, it's a simple CLI that demonstrates clipboard monitoring.
//!
//! ## Rust Learning: Clipboard Access
//!
//! The `arboard` crate provides cross-platform clipboard access:
//! - `Clipboard::new()`: Create a clipboard instance
//! - `get_text()`: Read text from clipboard
//! - `set_text()`: Write text to clipboard
//! - `get_image()`: Read image from clipboard
//! - `set_image()`: Write image to clipboard

use std::time::Duration;

use anyhow::Result;
use arboard::Clipboard;
use tracing::info;

use clipsync_core::device::{Device, DeviceType};
use clipsync_core::ClipboardItem;

/// ## Rust Concept: `main` Function
///
/// Every Rust binary needs a `main` function as the entry point.
/// With `#[tokio::main]`, it can be async for non-blocking I/O.
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("clipsync_desktop=debug")
        .init();

    info!("ClipSync Desktop - Clipboard Monitor Demo");
    info!("==========================================");

    // Create our device identity
    let device = Device::new("My Desktop").with_type(DeviceType::Desktop);
    info!("Device ID: {}", device.id);
    info!("Platform: {}", device.platform);

    // Create clipboard access
    // ## Rust Concept: `?` Error Propagation
    //
    // If `Clipboard::new()` returns an error, `?` will:
    // 1. Return early from this function
    // 2. Convert the error to our return type (anyhow::Error)
    let mut clipboard = Clipboard::new()?;

    info!("\nMonitoring clipboard for changes...");
    info!("Copy some text to see it here. Press Ctrl+C to exit.\n");

    // Keep track of the last clipboard content to detect changes
    let mut last_content: Option<String> = None;

    // ## Rust Concept: Infinite Loops
    //
    // `loop` creates an infinite loop. Use `break` to exit.
    // Here we poll the clipboard every 500ms to detect changes.
    //
    // NOTE: A production app would use platform-specific clipboard
    // change notifications instead of polling. This is just a demo.
    loop {
        // Try to read text from clipboard
        match clipboard.get_text() {
            Ok(text) => {
                // Check if it's different from last time
                let is_new = match &last_content {
                    Some(last) => last != &text,
                    None => true, // First time seeing content
                };

                if is_new {
                    info!("📋 Clipboard changed!");

                    // Create a ClipboardItem
                    let item = ClipboardItem::text(&text, device.id.clone());

                    // Log some info about it
                    info!("  ID: {}", item.id);
                    info!("  Content: {}", truncate(&text, 100));
                    info!("  Size: {} bytes", item.content.size());
                    info!("  Timestamp: {}", item.timestamp);
                    info!("");

                    // Remember this content
                    last_content = Some(text);
                }
            }
            Err(_) => {
                // Clipboard might be empty or contain non-text data
                // This is normal, just ignore
            }
        }

        // Wait before checking again
        // ## Rust Concept: `tokio::time::sleep`
        //
        // This is an async sleep - it doesn't block the thread.
        // Other tasks can run while we're waiting.
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Truncates a string to a maximum length, adding "..." if truncated.
///
/// ## Rust Concept: Slicing Strings
///
/// Rust strings are UTF-8, so you can't just slice by byte index
/// (you might cut in the middle of a character!).
///
/// `.chars()` gives an iterator of characters, which we can take N of.
fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = chars[..max_len].iter().collect();
        format!("{}...", truncated)
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 5), "hello...");
    }

    #[test]
    fn test_truncate_unicode() {
        // Make sure we don't break Unicode characters
        assert_eq!(truncate("こんにちは", 3), "こんに...");
    }
}
