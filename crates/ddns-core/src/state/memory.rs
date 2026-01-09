// # Memory State Store
//
// In-memory implementation of StateStore.
//
// ## Purpose
//
// Provides a simple, fast state store that doesn't persist across restarts.
// Useful for testing, containerized deployments with restarts, or scenarios
// where persistence isn't critical.
//
// ## Crash Behavior
//
// - All state is lost on restart/crash
// - First run after crash will treat all IPs as "new" (will update DNS)
// - No recovery possible (state is in-memory only)
//
// ## When to Use
//
// - Testing environments
// - Container deployments where restart is acceptable
// - Scenarios where initial DNS update is harmless

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

use crate::traits::state_store::{StateStore, StateRecord};
use crate::Error;

/// In-memory state store implementation
///
/// This implementation stores all state in a HashMap protected by a RwLock.
/// It provides no persistence across restarts.
///
/// # Example
///
/// ```rust,no_run
/// use ddns_core::state::MemoryStateStore;
/// use ddns_core::traits::state_store::StateStore;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let store = MemoryStateStore::new();
///
///     // Set IP
///     store.set_last_ip("example.com", "1.2.3.4".parse()?).await?;
///
///     // Get IP
///     let ip = store.get_last_ip("example.com").await?;
///     assert_eq!(ip, Some("1.2.3.4".parse()?));
///
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MemoryStateStore {
    inner: Arc<RwLock<HashMap<String, StateRecord>>>,
}

impl MemoryStateStore {
    /// Create a new empty memory state store
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the number of records in the store
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Check if the store is empty
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }

    /// Clear all records from the store
    pub async fn clear(&self) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        guard.clear();
        Ok(())
    }
}

impl Default for MemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateStore for MemoryStateStore {
    async fn get_last_ip(&self, record_name: &str) -> Result<Option<IpAddr>, Error> {
        let guard = self.inner.read().await;
        Ok(guard.get(record_name).map(|record| record.last_ip))
    }

    async fn get_record(&self, record_name: &str) -> Result<Option<StateRecord>, Error> {
        let guard = self.inner.read().await;
        Ok(guard.get(record_name).cloned())
    }

    async fn set_last_ip(&self, record_name: &str, ip: IpAddr) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        let record = StateRecord::new(ip);
        guard.insert(record_name.to_string(), record);
        Ok(())
    }

    async fn set_record(&self, record_name: &str, record: &StateRecord) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        guard.insert(record_name.to_string(), record.clone());
        Ok(())
    }

    async fn delete_record(&self, record_name: &str) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        guard.remove(record_name);
        Ok(())
    }

    async fn list_records(&self) -> Result<Vec<String>, Error> {
        let guard = self.inner.read().await;
        Ok(guard.keys().cloned().collect())
    }

    async fn flush(&self) -> Result<(), Error> {
        // No-op for memory store (everything is already "persisted")
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_store_basic() {
        let store = MemoryStateStore::new();

        // Initially empty
        assert!(store.is_empty().await);
        assert_eq!(store.len().await, 0);

        // Set and get
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        store.set_last_ip("example.com", ip).await.unwrap();

        assert_eq!(store.len().await, 1);
        assert!(!store.is_empty().await);

        let retrieved = store.get_last_ip("example.com").await.unwrap();
        assert_eq!(retrieved, Some(ip));

        // Delete
        store.delete_record("example.com").await.unwrap();
        assert_eq!(store.len().await, 0);
    }

    #[tokio::test]
    async fn test_memory_store_record() {
        let store = MemoryStateStore::new();

        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let record = StateRecord::new(ip);

        store.set_record("example.com", &record).await.unwrap();

        let retrieved = store.get_record("example.com").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().last_ip, ip);
    }

    #[tokio::test]
    async fn test_memory_store_list() {
        let store = MemoryStateStore::new();

        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();

        store.set_last_ip("example.com", ip1).await.unwrap();
        store.set_last_ip("test.com", ip2).await.unwrap();

        let records = store.list_records().await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.contains(&"example.com".to_string()));
        assert!(records.contains(&"test.com".to_string()));
    }
}
