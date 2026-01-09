// # IP Source Trait
//
// Defines the interface for detecting and monitoring IP address changes.
//
// ## Implementations
//
// - Netlink-based (Linux): `ddns-ip-netlink` crate
// - Future: UDP socket, HTTP-based, platform-specific APIs
//
// ## Usage
//
// ```rust,ignore
// use ddns_core::IpSource;
// use tokio_stream::StreamExt;
//
// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let source = /* IpSource implementation */;
//
//     // Get current IP
//     let current_ip = source.current().await?;
//
//     // Watch for changes
//     let mut stream = source.watch();
//     while let Some(change) = stream.next().await {
//         println!("IP changed: {:?}", change);
//     }
//
//     Ok(())
// }
// ```

use async_trait::async_trait;
use tokio_stream::Stream;
use std::net::IpAddr;
use std::pin::Pin;

/// Represents a detected IP address change event
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpChangeEvent {
    /// The new IP address
    pub new_ip: IpAddr,
    /// The previous IP address (if known)
    pub previous_ip: Option<IpAddr>,
    /// Which IP version this change affects
    pub version: IpVersion,
}

/// IP version (v4 or v6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpVersion {
    V4,
    V6,
}

impl IpChangeEvent {
    /// Create a new IP change event
    ///
    /// This constructor is public for use in:
    /// - `IpSource` implementations
    /// - Contract tests within ddns-core
    /// - External testing code
    pub fn new(new_ip: IpAddr, previous_ip: Option<IpAddr>) -> Self {
        let version = match new_ip {
            IpAddr::V4(_) => IpVersion::V4,
            IpAddr::V6(_) => IpVersion::V6,
        };

        Self {
            new_ip,
            previous_ip,
            version,
        }
    }
}

/// Trait for IP source implementations
///
/// This trait defines two core capabilities:
/// 1. **current()**: Fetch the current IP address
/// 2. **watch()**: Stream of IP change events
///
/// Implementations must be thread-safe and usable across async tasks.
///
/// # Trust Level: Semi-Trusted
///
/// IP sources are **semi-trusted** components with the following capabilities:
///
/// ## Allowed Capabilities
/// - ✅ Perform platform-specific I/O (Netlink, sockets, sysfs, /proc)
/// - ✅ Allocate bounded memory for streams and events
/// - ⚠️ Spawn tasks ONLY for event monitoring (not polling loops)
///
/// ## Forbidden Capabilities
/// - ❌ Perform DNS updates (use `DnsProvider`)
/// - ❌ Access state store directly (use `DdnsEngine`)
/// - ❌ Implement retry logic (use `DdnsEngine`)
/// - ❌ Spawn polling loops with `sleep()` (use event-driven mechanisms)
/// - ❌ Make decisions about when to update DNS
///
/// ## Rationale
///
/// IP sources need platform-specific I/O access to detect network changes,
/// but must not cross into business logic or coordination responsibilities.
/// They are **observers**, not **decision-makers**.
///
/// ## Task Spawning Rules
///
/// If you spawn tasks in your implementation:
/// - Task MUST wait for events (Netlink, socket notifications), not poll periodically
/// - Task MUST have clear shutdown path (cancellation-safe)
/// - Task MUST NOT use `tokio::time::sleep()` or equivalent polling mechanisms
///
/// ## Examples
///
/// ✅ **CORRECT**: Event-driven with Netlink
/// ```rust,ignore
/// tokio::spawn(async move {
///     let socket = netlink_socket();
///     socket.subscribe_to_route_changes(); // Event-driven
///     loop {
///         let event = socket.read_event().await; // Wait for event
///         tx.emit(event);
///     }
/// });
/// ```
///
/// ❌ **WRONG**: Polling loop
/// ```rust,ignore
/// tokio::spawn(async move {
///     loop {
///         let ip = get_current_ip().await;
///         tx.emit(ip);
///         tokio::time::sleep(Duration::from_secs(60)).await; // WRONG!
///     }
/// });
/// ```
///
/// See `docs/architecture/TRUST_LEVELS.md` for complete trust level definitions.
#[async_trait]
pub trait IpSource: Send + Sync {
    /// Get the current IP address
    ///
    /// This method should return immediately with the current IP,
    /// without waiting for any changes.
    ///
    /// # Returns
    ///
    /// - `Ok(IpAddr)`: The current IP address
    /// - `Err(Error)`: If unable to determine the current IP
    async fn current(&self) -> Result<IpAddr, crate::Error>;

    /// Watch for IP changes
    ///
    /// Returns a stream that yields `IpChangeEvent` whenever the IP address changes.
    /// The stream should run indefinitely and never terminate under normal conditions.
    ///
    /// # Behavior
    ///
    /// - Should yield the initial IP immediately when first polled
    /// - Should yield subsequent events only when the IP actually changes
    /// - Must be cancellation-safe (dropping the stream cleans up resources)
    ///
    /// # Returns
    ///
    /// A pinned boxed stream of `IpChangeEvent` items
    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>>;

    /// Get the IP version this source monitors
    ///
    /// Some implementations may monitor only v4 or only v6.
    /// Returns `None` if the implementation supports both (dual-stack).
    fn version(&self) -> Option<IpVersion> {
        None
    }
}

/// Helper trait for constructing IP sources from configuration
pub trait IpSourceFactory: Send + Sync {
    /// Create an IpSource instance from configuration
    ///
    /// # Parameters
    ///
    /// - `config`: Configuration specific to this IP source type
    ///
    /// # Returns
    ///
    /// A boxed IpSource trait object
    fn create(
        &self,
        config: &crate::config::IpSourceConfig,
    ) -> Result<Box<dyn IpSource>, crate::Error>;
}
