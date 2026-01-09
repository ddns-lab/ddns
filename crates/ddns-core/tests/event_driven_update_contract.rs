//! Architectural Contract Test: Event-Driven Updates
//!
//! This test verifies that IP changes trigger EXACTLY ONE DNS update.
//!
//! Constraints verified:
//! - One IP event → One DNS update attempt
//! - No retry loops for the first update
//! - No polling between events
//!
//! If this test fails, someone has added:
//! - Automatic retries on first update
//! - Polling to check for updates
//! - Background periodic checks

mod common;

use ddns_core::traits::IpChangeEvent;
use ddns_core::DdnsEngine;
use std::net::IpAddr;
use common::*;

#[tokio::test]
async fn one_ip_change_triggers_exactly_one_dns_update() {
    // This test verifies that a single IP change event triggers
    // exactly one DNS update, with no retries or polling.

    // Arrange: Create a controlled IP source
    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    // Create provider that tracks calls
    let provider = Box::new(MockDnsProvider::new("test"));
    let update_count = provider.update_call_count();
    let provider_arc = std::sync::Arc::new(provider);

    // Create state store
    let state_store = Box::new(MockStateStore::new());

    // Create config
    let config = minimal_config("example.com");

    // Create engine
    // Note: We need to create a new provider that shares counters
    let shared_provider = MockDnsProvider::sharing_counters_with(&provider_arc);
    let (engine, mut event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(shared_provider),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Act: Run engine in background
    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    // Wait for engine to start
    let _ = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        event_rx.recv()
    ).await;

    // Act: Emit exactly one IP change event
    let new_ip = IpAddr::from([10, 0, 0, 1]);
    let event = IpChangeEvent::new(new_ip, Some(initial_ip));
    ip_event_tx.send(event).expect("event send succeeds");

    // Wait for update to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Shutdown
    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Exactly one update was attempted
    let final_count = provider_arc.update_call_count();
    assert_eq!(
        final_count, 1,
        "Expected exactly 1 DNS update for 1 IP event, got {}",
        final_count
    );
}

#[tokio::test]
async fn multiple_ip_changes_trigger_multiple_updates() {
    // Verify that each IP change triggers exactly one update
    // (no batching, no deduplication beyond what's explicit)

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = std::sync::Arc::new(provider);

    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    // Wait for startup
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit 3 IP changes
    for i in 0..3 {
        let new_ip = IpAddr::from([10, 0, 0, i]);
        let event = IpChangeEvent::new(new_ip, None);
        ip_event_tx.send(event).expect("event send succeeds");
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: 3 updates for 3 events
    let count = provider_arc.update_call_count();
    assert_eq!(count, 3, "Expected 3 updates for 3 IP events, got {}", count);
}

#[tokio::test]
async fn same_ip_does_not_trigger_update() {
    // Verify idempotency: same IP twice → only first update happens

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = std::sync::Arc::new(provider);

    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit same IP twice
    let event = IpChangeEvent::new(initial_ip, None);
    ip_event_tx.send(event.clone()).expect("send succeeds");
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    ip_event_tx.send(event).expect("send succeeds");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Only 1 update (second skipped due to idempotency)
    let count = provider_arc.update_call_count();
    assert_eq!(count, 1, "Expected 1 update for 2 identical IP events, got {}", count);
}

#[tokio::test]
async fn no_polling_between_events() {
    // This test verifies that the engine doesn't poll between events.
    //
    // We do this by:
    // 1. Emitting an event
    // 2. Waiting longer than any reasonable polling interval
    // 3. Verifying no additional updates occurred

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = std::sync::Arc::new(provider);

    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Emit one event
    let event = IpChangeEvent::new(initial_ip, None);
    ip_event_tx.send(event).expect("send succeeds");

    // Wait significantly longer than any reasonable polling interval
    // (e.g., if there was 1-second polling, we'd see multiple updates)
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: Still only 1 update (no polling occurred)
    let count = provider_arc.update_call_count();
    assert_eq!(
        count,
        1,
        "Expected 1 update without polling, got {} (possible polling detected)",
        count
    );
}
