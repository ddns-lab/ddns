//! Architectural Contract Test: State Model & Idempotency
//!
//! This test verifies that the state model ensures idempotency and
//! crash recovery.
//!
//! Constraints verified:
//! - State is persisted after successful DNS updates
//! - Idempotency prevents duplicate DNS updates
//! - Engine behavior is deterministic across restarts
//!
//! If this test fails, state management is broken.

mod common;

use common::*;
use ddns_core::DdnsEngine;
use ddns_core::traits::IpChangeEvent;
use ddns_core::traits::StateStore;
use std::net::IpAddr;

#[tokio::test]
async fn duplicate_ip_does_not_trigger_dns_update() {
    // Verify idempotency: same IP twice → only one DNS update

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    let provider = Box::new(MockDnsProvider::new("test"));
    let provider_arc = std::sync::Arc::new(provider);

    // Use a state store that persists state
    let state_store = Box::new(MockStateStore::new());

    let config = minimal_config("example.com");

    // Create and run engine
    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle =
        tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

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
    assert_eq!(
        count, 1,
        "Expected 1 update for 2 identical IP events, got {}",
        count
    );
}

#[tokio::test]
async fn restart_simulation_no_duplicate_updates() {
    // Simulate engine restart: verify state prevents duplicate updates

    let initial_ip = IpAddr::from([192, 168, 1, 1]);

    // First "run": Update DNS and persist state
    {
        let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);
        let provider = Box::new(MockDnsProvider::new("test"));
        let provider_arc = std::sync::Arc::new(provider);

        // State store that will persist across "restarts"
        let state_store = Box::new(MockStateStore::new());
        let state_store_arc = std::sync::Arc::new(state_store);

        let config = minimal_config("example.com");

        let (engine, _event_rx) = DdnsEngine::new(
            Box::new(ip_source),
            Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
            Box::new(MockStateStore::sharing_counters_with(&state_store_arc)),
            config,
        )
        .expect("engine construction succeeds");

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let engine_handle =
            tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Emit IP event (first run)
        let event = IpChangeEvent::new(initial_ip, None);
        ip_event_tx.send(event).expect("send succeeds");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        shutdown_tx.send(()).unwrap();
        engine_handle.await.unwrap().unwrap();

        // Verify: State was persisted
        let last_ip = state_store_arc.get_last_ip("example.com").await.unwrap();
        assert_eq!(last_ip, Some(initial_ip), "State should persist last IP");

        // Verify: DNS was updated once
        assert_eq!(
            provider_arc.update_call_count(),
            1,
            "First run should update DNS once"
        );
    }

    // Second "run": Same IP, should skip update due to state
    {
        let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);
        let provider = Box::new(MockDnsProvider::new("test"));
        let provider_arc = std::sync::Arc::new(provider);

        // Create a new state store (simulates restart)
        let state_store = Box::new(MockStateStore::new());
        let state_store_arc = std::sync::Arc::new(state_store);
        let config = minimal_config("example.com");

        // Pre-populate state with the IP from first run
        state_store_arc
            .set_last_ip("example.com", initial_ip)
            .await
            .unwrap();

        let (engine, _event_rx) = DdnsEngine::new(
            Box::new(ip_source),
            Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
            Box::new(MockStateStore::sharing_counters_with(&state_store_arc)),
            config,
        )
        .expect("engine construction succeeds");

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let engine_handle =
            tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Emit same IP event (second run)
        let event = IpChangeEvent::new(initial_ip, None);
        ip_event_tx.send(event).expect("send succeeds");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        shutdown_tx.send(()).unwrap();
        engine_handle.await.unwrap().unwrap();

        // Verify: DNS was NOT updated (idempotency from state)
        assert_eq!(
            provider_arc.update_call_count(),
            0,
            "Second run should skip update (state exists)"
        );
    }
}

#[tokio::test]
async fn ip_change_after_restart_triggers_update() {
    // Verify that IP change AFTER restart triggers new update

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let new_ip = IpAddr::from([10, 0, 0, 1]);

    // First run
    {
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

        let engine_handle =
            tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let event = IpChangeEvent::new(initial_ip, None);
        ip_event_tx.send(event).expect("send succeeds");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        shutdown_tx.send(()).unwrap();
        engine_handle.await.unwrap().unwrap();
    }

    // Second run with different IP
    {
        let (ip_source, ip_event_tx) = ControlledIpSource::new(new_ip);
        let provider = Box::new(MockDnsProvider::new("test"));
        let provider_arc = std::sync::Arc::new(provider);

        // Pre-populate state with old IP
        let state_store = Box::new(MockStateStore::new());
        let state_store_arc = std::sync::Arc::new(state_store);
        state_store_arc
            .set_last_ip("example.com", initial_ip)
            .await
            .unwrap();

        let config = minimal_config("example.com");

        let (engine, _event_rx) = DdnsEngine::new(
            Box::new(ip_source),
            Box::new(MockDnsProvider::sharing_counters_with(&provider_arc)),
            Box::new(MockStateStore::sharing_counters_with(&state_store_arc)),
            config,
        )
        .expect("engine construction succeeds");

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let engine_handle =
            tokio::spawn(async move { engine.run_with_shutdown(Some(shutdown_rx)).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Emit NEW IP event
        let event = IpChangeEvent::new(new_ip, Some(initial_ip));
        ip_event_tx.send(event).expect("send succeeds");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        shutdown_tx.send(()).unwrap();
        engine_handle.await.unwrap().unwrap();

        // Verify: DNS WAS updated (IP changed)
        assert_eq!(
            provider_arc.update_call_count(),
            1,
            "Second run with new IP should update DNS"
        );

        // Verify: State was updated
        let last_ip = state_store_arc.get_last_ip("example.com").await.unwrap();
        assert_eq!(last_ip, Some(new_ip), "State should reflect new IP");
    }
}
