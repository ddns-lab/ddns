//! Test doubles and common utilities for architecture contract tests
//!
//! This module provides minimal test doubles that verify architectural
//! constraints without implementing real functionality.

use ddns_core::error::Result;
use ddns_core::traits::{
    DnsProvider, IpChangeEvent, IpSource, RecordMetadata, StateStore, UpdateResult,
};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};

/// A controlled IpSource that can emit events on demand
pub struct ControlledIpSource {
    /// Sender for test to send events
    test_tx: mpsc::UnboundedSender<IpChangeEvent>,
    /// Receiver for the engine's watch stream
    engine_rx: Arc<std::sync::Mutex<Option<mpsc::UnboundedReceiver<IpChangeEvent>>>>,
    /// Current IP to return
    current_ip: IpAddr,
    /// Call counter for current()
    current_call_count: Arc<AtomicUsize>,
    /// Call counter for watch()
    watch_call_count: Arc<AtomicUsize>,
}

impl ControlledIpSource {
    /// Create a new controlled IP source
    pub fn new(current_ip: IpAddr) -> (Self, mpsc::UnboundedSender<IpChangeEvent>) {
        let (test_tx, engine_rx) = mpsc::unbounded_channel();

        let source = Self {
            test_tx: test_tx.clone(),
            engine_rx: Arc::new(std::sync::Mutex::new(Some(engine_rx))),
            current_ip,
            current_call_count: Arc::new(AtomicUsize::new(0)),
            watch_call_count: Arc::new(AtomicUsize::new(0)),
        };

        (source, test_tx)
    }

    /// Get the number of times current() was called
    pub fn current_call_count(&self) -> usize {
        self.current_call_count.load(Ordering::SeqCst)
    }

    /// Get the number of times watch() was called
    pub fn watch_call_count(&self) -> usize {
        self.watch_call_count.load(Ordering::SeqCst)
    }

    /// Emit an IP change event (convenience method for tests)
    pub fn emit_event(&self, event: IpChangeEvent) {
        let _ = self.test_tx.send(event);
    }
}

#[async_trait::async_trait]
impl IpSource for ControlledIpSource {
    async fn current(&self) -> Result<IpAddr> {
        self.current_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.current_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        self.watch_call_count.fetch_add(1, Ordering::SeqCst);

        // Take the receiver (only called once)
        let rx = self
            .engine_rx
            .lock()
            .unwrap()
            .take()
            .expect("watch() can only be called once");

        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        Box::pin(stream)
    }
}

/// An IP source that never emits events (for idle testing)
pub struct IdleIpSource {
    current_ip: IpAddr,
}

impl IdleIpSource {
    pub fn new(current_ip: IpAddr) -> Self {
        Self { current_ip }
    }
}

#[async_trait::async_trait]
impl IpSource for IdleIpSource {
    async fn current(&self) -> Result<IpAddr> {
        Ok(self.current_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        // Create a channel but never send anything
        let (_tx, rx) = mpsc::unbounded_channel();
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

        Box::pin(stream)
    }
}

/// A mock DnsProvider that tracks calls
pub struct MockDnsProvider {
    /// Call counter for update_record()
    update_call_count: Arc<AtomicUsize>,
    /// Recorded record names from update calls
    updated_records: Arc<std::sync::Mutex<Vec<String>>>,
    /// Provider name
    pub name: &'static str,
}

impl MockDnsProvider {
    pub fn new(name: &'static str) -> Self {
        Self {
            update_call_count: Arc::new(AtomicUsize::new(0)),
            updated_records: Arc::new(std::sync::Mutex::new(Vec::new())),
            name,
        }
    }

    /// Get the number of times update_record() was called
    pub fn update_call_count(&self) -> usize {
        self.update_call_count.load(Ordering::SeqCst)
    }

    /// Get the list of records that were updated
    pub fn updated_records(&self) -> Vec<String> {
        self.updated_records.lock().unwrap().clone()
    }

    /// Create a new MockDnsProvider that shares counters with an existing one
    pub fn sharing_counters_with(other: &Self) -> Self {
        Self {
            update_call_count: Arc::clone(&other.update_call_count),
            updated_records: Arc::clone(&other.updated_records),
            name: other.name,
        }
    }
}

#[async_trait::async_trait]
impl DnsProvider for MockDnsProvider {
    async fn update_record(&self, record_name: &str, new_ip: IpAddr) -> Result<UpdateResult> {
        self.update_call_count.fetch_add(1, Ordering::SeqCst);
        self.updated_records
            .lock()
            .unwrap()
            .push(record_name.to_string());

        Ok(UpdateResult::Updated {
            previous_ip: None,
            new_ip,
        })
    }

    async fn get_record(&self, record_name: &str) -> Result<RecordMetadata> {
        Ok(RecordMetadata {
            id: "test-id".to_string(),
            name: record_name.to_string(),
            ip: IpAddr::from([0, 0, 0, 0]),
            ttl: Some(300),
            extra: serde_json::json!({}),
        })
    }

    fn supports_record(&self, _record_name: &str) -> bool {
        true
    }

    fn provider_name(&self) -> &'static str {
        self.name
    }
}

/// A mock StateStore that tracks calls
pub struct MockStateStore {
    /// Call counter for get_last_ip()
    get_call_count: Arc<AtomicUsize>,
    /// Call counter for set_last_ip()
    set_call_count: Arc<AtomicUsize>,
    /// Call counter for flush()
    flush_call_count: Arc<AtomicUsize>,
    /// Stored IPs
    state: Arc<std::sync::Mutex<std::collections::HashMap<String, IpAddr>>>,
}

impl MockStateStore {
    pub fn new() -> Self {
        Self {
            get_call_count: Arc::new(AtomicUsize::new(0)),
            set_call_count: Arc::new(AtomicUsize::new(0)),
            flush_call_count: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Get the number of times get_last_ip() was called
    pub fn get_call_count(&self) -> usize {
        self.get_call_count.load(Ordering::SeqCst)
    }

    /// Get the number of times set_last_ip() was called
    pub fn set_call_count(&self) -> usize {
        self.set_call_count.load(Ordering::SeqCst)
    }

    /// Get the number of times flush() was called
    pub fn flush_call_count(&self) -> usize {
        self.flush_call_count.load(Ordering::SeqCst)
    }

    /// Create a new MockStateStore that shares counters with an existing one
    pub fn sharing_counters_with(other: &Self) -> Self {
        Self {
            get_call_count: Arc::clone(&other.get_call_count),
            set_call_count: Arc::clone(&other.set_call_count),
            flush_call_count: Arc::clone(&other.flush_call_count),
            state: Arc::clone(&other.state),
        }
    }
}

#[async_trait::async_trait]
impl StateStore for MockStateStore {
    async fn get_last_ip(&self, record_name: &str) -> Result<Option<IpAddr>> {
        self.get_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.state.lock().unwrap().get(record_name).copied())
    }

    async fn get_record(
        &self,
        _record_name: &str,
    ) -> Result<Option<ddns_core::traits::StateRecord>> {
        Ok(None)
    }

    async fn set_last_ip(&self, record_name: &str, ip: IpAddr) -> Result<()> {
        self.set_call_count.fetch_add(1, Ordering::SeqCst);
        self.state
            .lock()
            .unwrap()
            .insert(record_name.to_string(), ip);
        Ok(())
    }

    async fn set_record(
        &self,
        _record_name: &str,
        _record: &ddns_core::traits::StateRecord,
    ) -> Result<()> {
        Ok(())
    }

    async fn delete_record(&self, _record_name: &str) -> Result<()> {
        Ok(())
    }

    async fn list_records(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn flush(&self) -> Result<()> {
        self.flush_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Helper to create a minimal DdnsConfig for testing
pub fn minimal_config(record_name: &str) -> ddns_core::config::DdnsConfig {
    ddns_core::config::DdnsConfig {
        ip_source: ddns_core::config::IpSourceConfig::Netlink {
            interface: None,
            version: None,
        },
        provider: ddns_core::config::ProviderConfig::Cloudflare {
            api_token: "test-token".to_string(),
            zone_id: None,
            account_id: None,
        },
        state_store: ddns_core::config::StateStoreConfig::Memory,
        records: vec![ddns_core::config::RecordConfig::new(record_name)],
        engine: ddns_core::config::EngineConfig {
            max_retries: 3,
            retry_delay_secs: 1,
            startup_delay_secs: 0,
            min_update_interval_secs: 0, // Disabled for tests
            event_channel_capacity: 100,
            metadata: std::collections::HashMap::new(),
        },
    }
}
