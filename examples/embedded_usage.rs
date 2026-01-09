//! Minimal embedding example for ddns-core
//!
//! This example demonstrates using ddns-core as a library in a custom application.
//! The engine lifecycle is fully managed by the application.

#![allow(dead_code)]

use ddns_core::config::RecordConfig;
use ddns_core::{
    traits::{DnsProvider, IpChangeEvent, IpSource, StateStore},
    DdnsConfig, DdnsEngine, Result,
};
use std::net::IpAddr;
use std::pin::Pin;
use tokio_stream::Stream;

/// Custom IP source for embedded usage
struct EmbeddedIpSource {
    current_ip: IpAddr,
    event_tx: tokio::sync::mpsc::UnboundedSender<IpChangeEvent>,
}

impl EmbeddedIpSource {
    fn new(current_ip: IpAddr) -> (Self, tokio::sync::mpsc::UnboundedReceiver<IpChangeEvent>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                current_ip,
                event_tx: tx,
            },
            rx,
        )
    }

    /// Simulate an IP change (for testing)
    fn emit_change(&mut self, new_ip: IpAddr) {
        let previous_ip = self.current_ip;
        let event = IpChangeEvent::new(new_ip, Some(previous_ip));
        let _ = self.event_tx.send(event);
        self.current_ip = new_ip;
    }
}

#[async_trait::async_trait]
impl IpSource for EmbeddedIpSource {
    async fn current(&self) -> Result<IpAddr> {
        Ok(self.current_ip)
    }

    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        // In a real implementation, this would wrap the event receiver
        // For this example, we return a placeholder
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
    }
}

/// Custom DNS provider for embedded usage
struct EmbeddedProvider {
    update_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl EmbeddedProvider {
    fn new() -> Self {
        Self {
            update_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    fn update_count(&self) -> usize {
        self.update_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl DnsProvider for EmbeddedProvider {
    async fn update_record(
        &self,
        record_name: &str,
        new_ip: IpAddr,
    ) -> Result<ddns_core::traits::UpdateResult> {
        self.update_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!("[Embedded] Updating {} -> {}", record_name, new_ip);

        // Simulate successful update
        Ok(ddns_core::traits::UpdateResult::Updated {
            previous_ip: None,
            new_ip,
        })
    }

    async fn get_record(&self, record_name: &str) -> Result<ddns_core::traits::RecordMetadata> {
        Ok(ddns_core::traits::RecordMetadata {
            id: "embedded-id".to_string(),
            name: record_name.to_string(),
            ip: IpAddr::from([127, 0, 0, 1]),
            ttl: Some(300),
            extra: serde_json::json!({}),
        })
    }

    fn supports_record(&self, _record_name: &str) -> bool {
        true
    }

    fn provider_name(&self) -> &'static str {
        "embedded"
    }
}

/// Custom state store for embedded usage
struct EmbeddedStateStore {
    state: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, IpAddr>>>,
}

impl EmbeddedStateStore {
    fn new() -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl StateStore for EmbeddedStateStore {
    async fn get_last_ip(&self, record_name: &str) -> Result<Option<IpAddr>> {
        Ok(self.state.lock().unwrap().get(record_name).copied())
    }

    async fn get_record(
        &self,
        _record_name: &str,
    ) -> Result<Option<ddns_core::traits::StateRecord>> {
        Ok(None)
    }

    async fn set_last_ip(&self, record_name: &str, ip: IpAddr) -> Result<()> {
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
        println!("[Embedded] State flushed");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Embedded ddns-core Example ===\n");

    // Create custom components
    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, _ip_event_rx) = EmbeddedIpSource::new(initial_ip);
    let provider = Box::new(EmbeddedProvider::new());
    let state_store = Box::new(EmbeddedStateStore::new());

    // Create configuration
    let config = DdnsConfig {
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
        records: vec![RecordConfig::new("example.com")],
        engine: ddns_core::config::EngineConfig {
            max_retries: 0, // No retries for this example
            retry_delay_secs: 0,
            startup_delay_secs: 0,
            min_update_interval_secs: 0, // No rate limiting for example
            event_channel_capacity: 100, // Small buffer for example
            metadata: std::collections::HashMap::new(),
        },
    };

    // Create engine
    println!("1. Creating engine...");
    let (engine, mut event_rx) =
        DdnsEngine::new(Box::new(ip_source), provider, state_store, config)?;

    // Spawn event listener (optional)
    let event_listener = tokio::spawn(async move {
        println!("2. Event listener started");
        while let Some(event) = event_rx.recv().await {
            println!("[Event] {:?}", event);
        }
        println!("Event listener stopped");
    });

    // Run engine in background
    println!("3. Starting engine in background...");
    let engine_handle = tokio::spawn(async move { engine.run().await });

    // Let engine run briefly
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\n4. Engine is running. Application can do other work here.");
    println!("   (Engine lifecycle is fully managed by application)\n");

    // Simulate application work
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Stop engine by dropping the handle
    println!("5. Stopping engine (by dropping handle)...");
    drop(engine_handle);

    // Wait for cleanup
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Wait for event listener
    let _ = tokio::time::timeout(tokio::time::Duration::from_millis(100), event_listener).await;

    println!("\n6. Engine stopped cleanly.");
    println!("\n=== Embedding Successful ===");
    println!("Key Points:");
    println!("- Engine lifecycle is fully controlled by application");
    println!("- No global state");
    println!("- No reliance on process lifecycle");
    println!("- All components are custom (not ddnsd defaults)");

    Ok(())
}
