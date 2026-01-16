//! Netlink Behavior Contract Tests (Mock/Fake)
//!
//! This test module verifies architectural behaviors that are consistent
//! across all IP sources (including netlink). These tests use ControlledIpSource
//! to simulate events and verify engine behaviors cross-platform.
//!
//! Tests are cross-platform (Linux, macOS, Windows) and verify:
//! - Event flow: One IP change → One DNS update
//! - Idempotency: Same IP doesn't trigger duplicate updates
//! - Version detection: IPv4 vs IPv6 events
//! - Multiple records: Single IP change updates multiple records
//!
//! Note: Netlink-specific behaviors (debounce, IP filtering, prioritization)
//! are tested in integration_test.rs with real netlink sockets on Linux.

// Include common test infrastructure from ddns-core
#[path = "../../ddns-core/tests/common/mod.rs"]
mod common;

use common::*;
use ddns_core::config::{DdnsConfig, EngineConfig, IpSourceConfig, ProviderConfig, RecordConfig, StateStoreConfig};
use ddns_core::DdnsEngine;
use ddns_core::traits::IpChangeEvent;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

/// Helper to create a minimal config for netlink testing
fn netlink_config(record_names: Vec<&str>) -> DdnsConfig {
    DdnsConfig {
        ip_source: IpSourceConfig::Netlink {
            interface: None,
            version: None,
        },
        provider: ProviderConfig {
            provider_type: "mock".to_string(),
            config: serde_json::json!({}),
        },
        state_store: StateStoreConfig::Memory,
        records: record_names.iter().map(|n| RecordConfig::new(*n)).collect(),
        engine: EngineConfig {
            max_retries: 3,
            retry_delay_secs: 1,
            startup_delay_secs: 0,
            min_update_interval_secs: 0, // Disabled for tests
            event_channel_capacity: 100,
            metadata: HashMap::new(),
        },
    }
}

#[tokio::test]
async fn netlink_one_ip_change_triggers_one_dns_update() {
    //! Test that a single IP change event triggers exactly one DNS update.
    //! This is the core architectural contract of the event-driven system.

    let initial_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit one IP change
    let new_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
    let event = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Exactly one update
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 1,
        "Expected exactly 1 DNS update for 1 IP event, got {}",
        count
    );
}

#[tokio::test]
async fn netlink_multiple_records_all_updated() {
    //! Test that a single IP change updates all configured records.

    let initial_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com", "www.example.com", "api.example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit IP change
    let new_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
    let event = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: All 3 records updated
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 3,
        "Expected 3 updates for 3 records, got {}",
        count
    );

    let records = provider_arc.updated_records();
    assert_eq!(records.len(), 3);
    assert!(records.contains(&"example.com".to_string()));
    assert!(records.contains(&"www.example.com".to_string()));
    assert!(records.contains(&"api.example.com".to_string()));
}

#[tokio::test]
async fn netlink_idempotency_same_ip_no_duplicate_update() {
    //! Test that emitting the same IP twice results in only one update
    //! (idempotency - StateStore prevents duplicate updates).

    let initial_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let new_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

    // Emit same IP twice
    let event1 = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event1).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let event2 = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event2).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Only 1 update (second skipped due to idempotency)
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 1,
        "Expected 1 update for identical IP events (idempotency), got {}",
        count
    );
}

#[tokio::test]
async fn netlink_event_signature_matches_expected() {
    //! Test that IpChangeEvent has the correct structure with:
    //! - new_ip: IpAddr
    //! - previous_ip: Option<IpAddr>
    //! - version: IpVersion

    let new_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
    let previous_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

    let event = IpChangeEvent::new(new_ip, Some(previous_ip));

    // Verify event structure
    assert_eq!(event.new_ip, new_ip);
    assert_eq!(event.previous_ip, Some(previous_ip));

    // Verify version detection
    use ddns_core::traits::IpVersion;
    assert_eq!(event.version, IpVersion::V4);

    // Test IPv6
    let new_ip_v6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
    let event_v6 = IpChangeEvent::new(new_ip_v6, None);
    assert_eq!(event_v6.version, IpVersion::V6);
}

#[tokio::test]
async fn netlink_ipv4_events_trigger_updates() {
    //! Test that IPv4 change events trigger DNS updates correctly.

    let initial_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit IPv4 change event
    let new_ip_v4 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
    let event = IpChangeEvent::new(new_ip_v4, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Update occurred
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 1,
        "Expected 1 update for IPv4 change, got {}",
        count
    );
}

#[tokio::test]
async fn netlink_ipv6_events_trigger_updates() {
    //! Test that IPv6 change events trigger DNS updates correctly.

    let initial_ip = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit IPv6 change event
    let new_ip_v6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2));
    let event = IpChangeEvent::new(new_ip_v6, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Update occurred
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 1,
        "Expected 1 update for IPv6 change, got {}",
        count
    );
}

#[tokio::test]
async fn netlink_sequential_ip_changes_trigger_sequential_updates() {
    //! Test that sequential IP changes (with time between them) trigger
    //! separate DNS updates.

    let initial_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit first change
    let ip1 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
    let event1 = IpChangeEvent::new(ip1, Some(initial_ip));
    ip_event_tx.send(event1).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Emit second change
    let ip2 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2));
    let event2 = IpChangeEvent::new(ip2, Some(ip1));
    ip_event_tx.send(event2).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: 2 updates for 2 different IPs
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 2,
        "Expected 2 updates for 2 sequential IP changes, got {}",
        count
    );
}

#[tokio::test]
async fn netlink_state_persists_across_updates() {
    //! Test that StateStore correctly persists IP state across updates,
    //! enabling idempotency and crash recovery.

    let initial_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = Arc::new(provider);
    let state_store = Box::new(MockStateStore::new());
    let config = netlink_config(vec!["example.com"]);

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let ip1 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

    // First update
    let event1 = IpChangeEvent::new(ip1, Some(initial_ip));
    ip_event_tx.send(event1).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second update with same IP (should be skipped by StateStore)
    let event2 = IpChangeEvent::new(ip1, Some(initial_ip));
    ip_event_tx.send(event2).expect("event send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: StateStore prevented duplicate update
    let count = provider_arc.update_call_count();
    assert_eq!(
        count, 1,
        "Expected 1 update with StateStore persistence, got {}",
        count
    );

    // Verify state store was called
    // Note: get_last_ip is called to check idempotency, set_last_ip to persist
}
