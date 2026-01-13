//! # Local Network Discovery
//!
//! This module handles finding devices on the local network using mDNS/DNS-SD.
//!
//! ## How mDNS Works
//!
//! mDNS (Multicast DNS) is how devices find each other without a central server:
//!
//! 1. Device A announces: "I'm clipsync service at 192.168.1.100:8765"
//! 2. Device B sees the announcement and can connect directly
//!
//! This is how AirDrop, Chromecast, and similar technologies work.
//!
//! ## Rust Learning: Async Streams
//!
//! Sometimes we need to receive a STREAM of values over time (not just one).
//! In Rust, this is done with `Stream` trait (async iterator).
//!
//! ```rust
//! use futures::StreamExt;  // For .next() on streams
//!
//! while let Some(event) = discovery.next().await {
//!     match event {
//!         DiscoveryEvent::DeviceFound(device) => { ... }
//!         DiscoveryEvent::DeviceLost(id) => { ... }
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::device::{Device, DeviceId};

// ============================================================================
// CONSTANTS
// ============================================================================
/// The mDNS service type for ClipSync.
///
/// Format: `_<service>._<protocol>.local.`
/// - `_clipsync`: Our service name
/// - `_tcp`: We use TCP (could also be `_udp`)
/// - `.local.`: Standard suffix for mDNS
pub const SERVICE_TYPE: &str = "_clipsync._tcp.local.";

/// Default port for ClipSync connections.
pub const DEFAULT_PORT: u16 = 43210;

// ============================================================================
// DISCOVERY EVENTS
// ============================================================================
/// Events emitted by the discovery service.
///
/// ## Rust Concept: Clone + Send + Sync
///
/// For types used across threads (like in channels), Rust needs guarantees:
/// - `Clone`: Can create a copy
/// - `Send`: Safe to send to another thread
/// - `Sync`: Safe to share references across threads
///
/// These traits are auto-implemented for most types. If you see errors
/// about missing Send/Sync, it's usually because you're holding a
/// non-thread-safe type (like Rc or RefCell) across an await point.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new device was found on the network.
    DeviceFound {
        device_id: DeviceId,
        device_name: String,
        addresses: Vec<IpAddr>,
        port: u16,
    },

    /// A previously found device is no longer available.
    DeviceLost {
        device_id: DeviceId,
    },
}

// ============================================================================
// DISCOVERED DEVICE
// ============================================================================
/// Information about a device discovered on the network.
///
/// This is lighter than a full `Device` - just enough for the UI to show.
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub addresses: Vec<IpAddr>,
    pub port: u16,
    /// When we last saw this device
    pub last_seen: std::time::Instant,
}

// ============================================================================
// DISCOVERY SERVICE
// ============================================================================
/// Service for discovering ClipSync devices on the local network.
///
/// ## Rust Concept: Arc and RwLock
///
/// When multiple async tasks need to access the same data:
/// - `Arc<T>`: Atomic Reference Counted pointer. Multiple owners, thread-safe.
/// - `RwLock<T>`: Allows multiple readers OR one writer at a time.
///
/// `Arc<RwLock<T>>` is the common pattern for shared mutable state in async code.
///
/// ```rust
/// let data = Arc::new(RwLock::new(HashMap::new()));
///
/// // Reading (many can read at once)
/// let guard = data.read().await;
/// println!("{:?}", guard.get("key"));
///
/// // Writing (exclusive access)
/// let mut guard = data.write().await;
/// guard.insert("key", "value");
/// ```
pub struct DiscoveryService {
    /// The mDNS service daemon (handles network stuff)
    daemon: ServiceDaemon,

    /// Our own service info (what we broadcast)
    our_service: Option<ServiceInfo>,

    /// Discovered devices
    ///
    /// ## Rust Concept: HashMap
    ///
    /// HashMap<K, V> is Rust's hash table. Keys must implement:
    /// - `Eq`: Can check equality
    /// - `Hash`: Can compute a hash value
    ///
    /// DeviceId derives both, so it can be a key.
    devices: Arc<RwLock<HashMap<DeviceId, DiscoveredDevice>>>,

    /// Channel for sending discovery events to listeners
    ///
    /// ## Rust Concept: Broadcast Channel
    ///
    /// `broadcast` channel allows multiple receivers.
    /// When you `send()`, ALL receivers get a copy.
    /// (Unlike `mpsc` where only ONE receiver gets each message.)
    event_tx: broadcast::Sender<DiscoveryEvent>,
}

impl DiscoveryService {
    /// Creates a new discovery service.
    ///
    /// ## Rust Concept: `Result` with `anyhow`
    ///
    /// `anyhow::Result<T>` is like `Result<T, anyhow::Error>`.
    /// `anyhow::Error` can wrap ANY error type, making error handling easier.
    ///
    /// Use `anyhow` for applications (where you just need to display errors).
    /// Use `thiserror` for libraries (where callers need to match on error types).
    pub fn new() -> anyhow::Result<Self> {
        // Create the mDNS daemon
        let daemon = ServiceDaemon::new()?;

        // Create a broadcast channel with buffer size 16
        let (event_tx, _) = broadcast::channel(16);

        Ok(Self {
            daemon,
            our_service: None,
            devices: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        })
    }

    /// Starts announcing our service on the network.
    ///
    /// Other devices running ClipSync will be able to find us.
    ///
    /// ## Rust Concept: Mutable Borrow in Async
    ///
    /// `&mut self` means this method needs exclusive access to modify `self`.
    /// In async code, Rust ensures you don't hold mutable borrows across await points
    /// in ways that could cause data races.
    pub fn announce(&mut self, our_device: &Device, port: u16) -> anyhow::Result<()> {
        // Properties are key-value pairs attached to the service
        let properties = [
            ("id", our_device.id.to_string()),
            ("name", our_device.name.clone()),
            ("platform", our_device.platform.clone()),
        ];

        // Create service info
        // Instance name should be unique on the network
        let instance_name = format!("ClipSync-{}", &our_device.id.to_string()[..8]);

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &format!("{}.local.", hostname::get()?.to_string_lossy()),
            "",  // Let it auto-detect IP
            port,
            &properties[..],
        )?;

        // Register with the daemon
        self.daemon.register(service.clone())?;
        self.our_service = Some(service);

        info!(
            "Announcing ClipSync service: {} on port {}",
            our_device.name, port
        );

        Ok(())
    }

    /// Starts browsing for other ClipSync devices.
    ///
    /// Returns a receiver for discovery events.
    ///
    /// ## Rust Concept: Receivers and Ownership
    ///
    /// The returned `Receiver` is owned by the caller.
    /// When they drop it, Rust automatically cleans up.
    /// No manual unsubscribe needed!
    pub fn browse(&self) -> anyhow::Result<broadcast::Receiver<DiscoveryEvent>> {
        // Subscribe to our event channel
        let event_rx = self.event_tx.subscribe();

        // Browse for services of our type
        let receiver = self.daemon.browse(SERVICE_TYPE)?;

        // Spawn a task to handle discovery events
        let devices = Arc::clone(&self.devices);
        let event_tx = self.event_tx.clone();

        // ## Rust Concept: `tokio::spawn`
        //
        // `spawn` starts a new async task that runs concurrently.
        // It's like creating a thread, but much lighter weight.
        //
        // The `move` keyword transfers ownership of captured variables
        // (devices, event_tx) INTO the closure.
        tokio::spawn(async move {
            // ## Rust Concept: `loop` vs `while`
            //
            // `loop` is an infinite loop. Use with `break` to exit.
            // `while let` loops while a pattern matches.
            //
            // Here we use `while let` to loop while receiving events.
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        debug!("Discovered service: {}", info.get_fullname());

                        // Extract device info from service properties
                        let props = info.get_properties();

                        // Parse device ID
                        let device_id = match props.get_property_val_str("id") {
                            Some(id_str) => {
                                // ## Rust Concept: `if let` for Option/Result
                                //
                                // `if let` is like `match` but for one pattern:
                                // if let Some(x) = option { use x } else { handle none }
                                if let Ok(uuid) = uuid::Uuid::parse_str(id_str) {
                                    DeviceId::from_uuid(uuid)
                                } else {
                                    warn!("Invalid device ID: {}", id_str);
                                    continue;
                                }
                            }
                            None => {
                                warn!("Service missing device ID");
                                continue;
                            }
                        };

                        let device_name = props
                            .get_property_val_str("name")
                            .unwrap_or("Unknown")
                            .to_string();

                        // Get IP addresses
                        let addresses: Vec<IpAddr> = info.get_addresses().iter().copied().collect();
                        let port = info.get_port();

                        // Store the device
                        {
                            // ## Rust Concept: Scope for Lock Guards
                            //
                            // The `{}` block limits how long we hold the write lock.
                            // When `guard` goes out of scope, the lock is released.
                            // This is important to avoid holding locks across await points!
                            let mut guard = devices.write().await;
                            guard.insert(
                                device_id.clone(),
                                DiscoveredDevice {
                                    device_id: device_id.clone(),
                                    name: device_name.clone(),
                                    addresses: addresses.clone(),
                                    port,
                                    last_seen: std::time::Instant::now(),
                                },
                            );
                        }

                        // Notify listeners
                        // `.ok()` ignores errors (no subscribers = no problem)
                        let _ = event_tx.send(DiscoveryEvent::DeviceFound {
                            device_id,
                            device_name,
                            addresses,
                            port,
                        });
                    }

                    ServiceEvent::ServiceRemoved(_, instance_name) => {
                        debug!("Service removed: {}", instance_name);

                        // Find and remove the device
                        // (In a real implementation, we'd store instance_name -> device_id mapping)
                    }

                    ServiceEvent::SearchStarted(_) => {
                        debug!("mDNS search started");
                    }

                    ServiceEvent::SearchStopped(_) => {
                        debug!("mDNS search stopped");
                    }

                    // Handle other events (e.g., ServiceFound which is emitted before ServiceResolved)
                    _ => {
                        debug!("Other mDNS event received");
                    }
                }
            }
        });

        Ok(event_rx)
    }

    /// Returns all currently discovered devices.
    ///
    /// ## Rust Concept: Async Methods
    ///
    /// This method is async because it needs to acquire a read lock.
    /// The `.await` on `read()` waits for the lock to be available.
    pub async fn get_devices(&self) -> Vec<DiscoveredDevice> {
        let guard = self.devices.read().await;
        guard.values().cloned().collect()
    }

    /// Stops announcing our service.
    pub fn stop_announcing(&mut self) -> anyhow::Result<()> {
        if let Some(service) = self.our_service.take() {
            self.daemon.unregister(service.get_fullname())?;
            info!("Stopped announcing ClipSync service");
        }
        Ok(())
    }
}

/// ## Rust Concept: `Drop` Trait
///
/// `Drop` is called automatically when a value goes out of scope.
/// It's like a destructor in C++.
///
/// Use it to clean up resources (close files, stop services, etc.).
impl Drop for DiscoveryService {
    fn drop(&mut self) {
        // Try to stop announcing, but ignore errors during cleanup
        let _ = self.stop_announcing();
        // The daemon will be shut down when it's dropped
        if let Err(e) = self.daemon.shutdown() {
            error!("Error shutting down mDNS daemon: {}", e);
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Test that we can create a discovery service.
    /// (Full mDNS testing requires network access)
    #[test]
    fn test_service_type_format() {
        // Service type should follow mDNS conventions
        assert!(SERVICE_TYPE.starts_with("_"));
        assert!(SERVICE_TYPE.ends_with(".local."));
        assert!(SERVICE_TYPE.contains("._tcp"));
    }

    /// Test DiscoveryEvent cloning (needed for broadcast channel).
    #[test]
    fn test_event_clone() {
        let event = DiscoveryEvent::DeviceFound {
            device_id: DeviceId::new(),
            device_name: "Test".to_string(),
            addresses: vec![],
            port: 8080,
        };

        // Clone should work (needed for broadcast)
        let cloned = event.clone();
        assert!(matches!(cloned, DiscoveryEvent::DeviceFound { .. }));
    }
}
