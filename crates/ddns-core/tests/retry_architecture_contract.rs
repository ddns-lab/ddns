//! Architectural Contract Test: Engine-Owned Retry Logic
//!
//! This test verifies that retry logic is explicitly configured and engine-owned,
//! not hidden in providers or triggered automatically.
//!
//! Constraints verified:
//! - Retries are controlled by explicit configuration (max_retries, retry_delay_secs)
//! - Retry logic lives in the engine, not in providers
//! - Retries don't interfere with event-driven IP monitoring
//! - Retries can be completely disabled via configuration
//!
//! Architectural boundaries (from AI_CONTRACT.md §2.3):
//! - ✅ ENGINE: Can implement retry policy (owner of orchestration)
//! - ❌ PROVIDER: Must NOT implement retry policy (only executes API calls)
//!
//! If this test fails, someone has moved retry logic to the wrong layer
//! or implemented automatic, hidden retry behavior.

mod common;

use common::*;
use ddns_core::DdnsEngine;
use ddns_core::traits::IpChangeEvent;
use std::net::IpAddr;

#[tokio::test]
async fn retries_can_be_disabled_via_config() {
    // Verify that retries are NOT automatic - they can be disabled
    // by setting max_retries = 0 in the configuration

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    // Create a provider that always fails
    struct FailingProvider {
        update_call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ddns_core::traits::DnsProvider for FailingProvider {
        async fn update_record(
            &self,
            _record_name: &str,
            _new_ip: IpAddr,
        ) -> ddns_core::Result<ddns_core::traits::UpdateResult> {
            self.update_call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(ddns_core::error::Error::Other(
                "Provider unavailable".to_string(),
            ))
        }

        async fn get_record(
            &self,
            _record_name: &str,
        ) -> ddns_core::Result<ddns_core::traits::RecordMetadata> {
            Ok(ddns_core::traits::RecordMetadata {
                id: "test".to_string(),
                name: _record_name.to_string(),
                ip: IpAddr::from([0, 0, 0, 0]),
                ttl: Some(300),
                extra: serde_json::json!({}),
            })
        }

        fn supports_record(&self, _record_name: &str) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "failing"
        }
    }

    let provider = Box::new(FailingProvider {
        update_call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });
    let provider_arc = std::sync::Arc::new(provider);

    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    // DISABLE retries completely
    let mut config = config;
    config.engine.max_retries = 0;

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(FailingProvider {
            update_call_count: std::sync::Arc::clone(&provider_arc.update_call_count),
        }),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit one IP event
    let new_ip = IpAddr::from([10, 0, 0, 1]);
    let event = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    shutdown_tx.send(()).unwrap();
    let _ = engine_handle.await.unwrap();

    // Assert: With max_retries=0, exactly ONE attempt should be made
    let final_count = provider_arc
        .update_call_count
        .load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(
        final_count, 1,
        "Expected exactly 1 DNS update attempt with max_retries=0, got {}.
         If retries cannot be disabled, the architecture violates explicit control.",
        final_count
    );
}

#[tokio::test]
async fn retries_honor_explicit_configuration() {
    // Verify that when retries ARE configured, they execute exactly as specified
    // (no more, no less)

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    struct CountingFailingProvider {
        update_call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ddns_core::traits::DnsProvider for CountingFailingProvider {
        async fn update_record(
            &self,
            _record_name: &str,
            _new_ip: IpAddr,
        ) -> ddns_core::Result<ddns_core::traits::UpdateResult> {
            self.update_call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(ddns_core::error::Error::Other(
                "Provider unavailable".to_string(),
            ))
        }

        async fn get_record(
            &self,
            _record_name: &str,
        ) -> ddns_core::Result<ddns_core::traits::RecordMetadata> {
            Ok(ddns_core::traits::RecordMetadata {
                id: "test".to_string(),
                name: _record_name.to_string(),
                ip: IpAddr::from([0, 0, 0, 0]),
                ttl: Some(300),
                extra: serde_json::json!({}),
            })
        }

        fn supports_record(&self, _record_name: &str) -> bool {
            true
        }

        fn provider_name(&self) -> &'static str {
            "counting_failing"
        }
    }

    let provider = Box::new(CountingFailingProvider {
        update_call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });
    let provider_arc = std::sync::Arc::new(provider);

    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    // Explicitly configure retries
    let mut config = config;
    config.engine.max_retries = 2; // 1 initial + 2 retries = 3 total
    config.engine.retry_delay_secs = 0; // No delay for faster test

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(CountingFailingProvider {
            update_call_count: std::sync::Arc::clone(&provider_arc.update_call_count),
        }),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit one event
    let new_ip = IpAddr::from([10, 0, 0, 1]);
    let event = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    // Wait for all retries to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    let _ = engine_handle.await.unwrap();

    // Assert: With max_retries=2, we expect 3 attempts (1 initial + 2 retries)
    let final_count = provider_arc
        .update_call_count
        .load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(
        final_count, 3,
        "Expected 3 update attempts (1 initial + 2 retries) with max_retries=2, got {}.
         This verifies that retry logic is working as configured.",
        final_count
    );
}
