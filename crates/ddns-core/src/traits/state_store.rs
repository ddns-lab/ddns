// # State Store Trait
//
// Defines the interface for persistent state management.
//
// ## Purpose
//
// The state store ensures idempotency by tracking:
// - The last known IP address for each record
// - Update timestamps
// - Provider-specific state
//
// This prevents unnecessary API calls and provides crash recovery.
//
// ## Implementations
//
// - File-based: JSON or TOML files
// - Future: SQLite, Redis, etc.
//
// ## Usage
//
// ```rust
// use ddns_core::StateStore;
// use std::net::IpAddr;
//
// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let store = /* StateStore implementation */;
//
//     // Check last known IP
//     let last_ip = store.get_last_ip("example.com").await?;
//
//     // Update after successful DNS update
//     store.set_last_ip("example.com", IpAddr::from([1, 2, 3, 4])).await?;
//
//     Ok(())
// }
// ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::IpAddr;

/// State record for a DNS entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StateRecord {
    /// The last known IP address
    pub last_ip: IpAddr,
    /// Timestamp of the last update
    pub last_updated: chrono::DateTime<chrono::Utc>,
    /// Provider-specific metadata
    pub provider_metadata: HashMap<String, serde_json::Value>,
}

impl StateRecord {
    /// Create a new state record
    ///
    /// # Visibility
    ///
    /// This is `pub(crate)` to prevent external creation of malformed state records.
    /// State records should only be created internally by the `DdnsEngine` or `StateStore`
    /// implementations during normal operations.
    pub(crate) fn new(last_ip: IpAddr) -> Self {
        Self {
            last_ip,
            last_updated: chrono::Utc::now(),
            provider_metadata: HashMap::new(),
        }
    }

    /// Check if the record is stale (older than given duration)
    pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
        let now = chrono::Utc::now();
        now.signed_duration_since(self.last_updated) > max_age
    }
}

/// Trait for state store implementations
///
/// This trait defines the interface for persistent state storage.
/// Implementations must be thread-safe and usable across async tasks.
///
/// # Thread Safety
///
/// All methods must be safe to call concurrently from multiple tasks.
///
/// # Trust Level: Trusted (Core Component)
///
/// State store implementations are **trusted** core components with the following capabilities:
///
/// ## Allowed Capabilities
/// - ✅ Perform I/O for persistent storage (files, databases, etc.)
/// - ✅ Allocate bounded memory for state management
/// - ✅ Implement locking/concurrency control for thread safety
/// - ✅ Cache state in memory for performance (with explicit flush)
///
/// ## Forbidden Capabilities
/// - ❌ Spawn background tasks without clear lifecycle (use async I/O instead)
/// - ❌ Implement business logic (owned by `DdnsEngine`)
/// - ❌ Perform DNS updates (owned by `DnsProvider`)
/// - ❌ Monitor IP changes (owned by `IpSource`)
/// - ❌ Decide when to update (owned by `DdnsEngine`)
///
/// ## Rationale
///
/// State stores need persistent I/O capabilities to ensure durability and idempotency.
/// As a core component, they are trusted to manage state safely but must not
/// encroach on business logic responsibilities.
///
/// ## Implementation Guidelines
///
/// - **Async I/O only**: Use async file/database operations, never blocking I/O
/// - **Explicit flush**: `flush()` must persist all pending changes
/// - **Thread-safe**: All methods must be safe to call concurrently
/// - **Minimal allocations**: Prefer in-place updates over copy-on-write where possible
/// - **No background tasks**: If you need periodic flushing, use a timer in `DdnsEngine` instead
///
/// See `docs/architecture/TRUST_LEVELS.md` for complete trust level definitions.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Get the last known IP for a record
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    ///
    /// # Returns
    ///
    /// - `Ok(Some(IpAddr))`: The last known IP
    /// - `Ok(None)`: No record found
    /// - `Err(Error)`: Storage error
    async fn get_last_ip(&self, record_name: &str) -> Result<Option<IpAddr>, crate::Error>;

    /// Get the full state record
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    ///
    /// # Returns
    ///
    /// - `Ok(Some(StateRecord))`: The full state record
    /// - `Ok(None)`: No record found
    /// - `Err(Error)`: Storage error
    async fn get_record(&self, record_name: &str) -> Result<Option<StateRecord>, crate::Error>;

    /// Set the last known IP for a record
    ///
    /// This should create or update the state record.
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    /// - `ip`: The new IP address
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Successfully updated
    /// - `Err(Error)`: Storage error
    async fn set_last_ip(&self, record_name: &str, ip: IpAddr) -> Result<(), crate::Error>;

    /// Update the full state record
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    /// - `record`: The state record to save
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Successfully updated
    /// - `Err(Error)`: Storage error
    async fn set_record(&self, record_name: &str, record: &StateRecord)
    -> Result<(), crate::Error>;

    /// Delete a state record
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Successfully deleted (or didn't exist)
    /// - `Err(Error)`: Storage error
    async fn delete_record(&self, record_name: &str) -> Result<(), crate::Error>;

    /// List all record names in the store
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<String>)`: List of record names
    /// - `Err(Error)`: Storage error
    async fn list_records(&self) -> Result<Vec<String>, crate::Error>;

    /// Persist any pending changes
    ///
    /// Some implementations may buffer writes. This ensures
    /// all changes are flushed to persistent storage.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Successfully flushed
    /// - `Err(Error)`: Storage error
    async fn flush(&self) -> Result<(), crate::Error>;
}

/// Helper trait for constructing state stores from configuration
pub trait StateStoreFactory: Send + Sync {
    /// Create a StateStore instance from configuration
    ///
    /// # Parameters
    ///
    /// - `config`: Configuration specific to this state store
    ///
    /// # Returns
    ///
    /// A boxed StateStore trait object
    fn create(&self, config: &serde_json::Value) -> Result<Box<dyn StateStore>, crate::Error>;
}
