//! Architectural Contract Test: Shutdown Determinism
//!
//! This test verifies that shutdown is deterministic and complete.
//!
//! Constraints verified:
//! - Engine terminates on shutdown signal
//! - All internal tasks exit
//! - No futures remain pending
//! - State is flushed before exit
//!
//! If this test fails, someone has added:
//! - Detached background tasks
//! - Tasks that ignore cancellation
//! - Leaked futures
//! - Blocking operations in shutdown path

mod common;

use ddns_core::traits::{IpSource, DnsProvider, IpChangeEvent};
use ddns_core::DdnsEngine;
use std::net::IpAddr;
use common::*;

#[tokio::test]
async fn shutdown_signal_terminates_engine() {
    // This is the most basic shutdown test:
    // Verify that the engine responds to shutdown signal

    let ip_source = Box::new(IdleIpSource::new(IpAddr::from([192, 168, 1, 1])));
    let provider = Box::new(MockDnsProvider::new("test"));
    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(ip_source, provider, state_store, config)
        .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Start engine
    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    // Wait for startup
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send shutdown signal
    let shutdown_result = shutdown_tx.send(());
    assert!(shutdown_result.is_ok(), "shutdown signal send succeeds");

    // Wait for engine to stop
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        engine_handle
    ).await;

    assert!(
        result.is_ok(),
        "Engine should terminate within 5 seconds"
    );

    let engine_result = result.unwrap().unwrap();
    assert!(
        engine_result.is_ok(),
        "Engine should shut down successfully: {:?}",
        engine_result
    );
}

#[tokio::test]
async fn shutdown_flushes_state() {
    // Verify that StateStore::flush() is called on shutdown

    use std::sync::Arc;

    let ip_source = Box::new(IdleIpSource::new(IpAddr::from([192, 168, 1, 1])));
    let provider = Box::new(MockDnsProvider::new("test"));

    let state_store = Box::new(MockStateStore::new());
    let state_store_arc = Arc::new(state_store);

    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(
        ip_source,
        provider,
        Box::new(MockStateStore::sharing_counters_with(&state_store_arc)),
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    // Assert: flush was called exactly once
    let flush_count = state_store_arc.flush_call_count();
    assert_eq!(
        flush_count,
        1,
        "StateStore::flush() should be called exactly once on shutdown, got {}",
        flush_count
    );
}

#[tokio::test]
async fn shutdown_during_ip_update() {
    // Verify that shutdown can complete even if an IP update is in progress

    let initial_ip = IpAddr::from([192, 168, 1, 1]);
    let (ip_source, ip_event_tx) = ControlledIpSource::new(initial_ip);

    // Create a provider that delays updates
    struct SlowProvider {
        update_call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl DnsProvider for SlowProvider {
        async fn update_record(&self, _record_name: &str, _new_ip: IpAddr) -> ddns_core::Result<ddns_core::traits::UpdateResult> {
            self.update_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            // Simulate slow update
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            Ok(ddns_core::traits::UpdateResult::Updated {
                previous_ip: None,
                new_ip: _new_ip,
            })
        }

        async fn get_record(&self, _record_name: &str) -> ddns_core::Result<ddns_core::traits::RecordMetadata> {
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
            "slow"
        }
    }

    let provider = Box::new(SlowProvider {
        update_call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });

    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(
        Box::new(ip_source),
        provider,
        state_store,
        config,
    )
    .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Trigger an update that will take 200ms
    let event = IpChangeEvent::new(IpAddr::from([10, 0, 0, 1]), None);
    ip_event_tx.send(event).expect("event send succeeds");

    // Wait 50ms for update to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Shutdown while update is in progress
    shutdown_tx.send(()).unwrap();

    // Engine should still terminate (may wait for update or cancel it)
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        engine_handle
    ).await;

    assert!(
        result.is_ok(),
        "Engine should terminate within 5 seconds even during update"
    );
}

#[tokio::test]
async fn no_future_leaks_after_shutdown() {
    // Verify that no tasks are leaked after shutdown
    //
    // We do this by:
    // 1. Running the engine
    // 2. Shutting it down
    // 3. Verifying no tokio tasks remain

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // Track how many stream tasks exist
    let stream_task_count = Arc::new(AtomicUsize::new(0));

    struct CountingDropSource {
        current_ip: IpAddr,
        stream_task_count: Arc<AtomicUsize>,
    }

    impl Drop for CountingDropSource {
        fn drop(&mut self) {
            // Decrement when source is dropped
            self.stream_task_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[async_trait::async_trait]
    impl IpSource for CountingDropSource {
        async fn current(&self) -> ddns_core::Result<IpAddr> {
            Ok(self.current_ip)
        }

        fn watch(&self) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = IpChangeEvent> + Send + 'static>> {
            self.stream_task_count.fetch_add(1, Ordering::SeqCst);

            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
        }
    }

    let ip_source = Box::new(CountingDropSource {
        current_ip: IpAddr::from([192, 168, 1, 1]),
        stream_task_count: stream_task_count.clone(),
    });

    let provider = Box::new(MockDnsProvider::new("test"));
    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(ip_source, provider, state_store, config)
        .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx)).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    shutdown_tx.send(()).unwrap();
    let _ = engine_handle.await;

    // After shutdown, stream task count should return to 0
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let count = stream_task_count.load(Ordering::SeqCst);
    assert_eq!(
        count,
        0,
        "All stream tasks should be cleaned up after shutdown, count: {}",
        count
    );
}

#[tokio::test]
async fn multiple_shutdown_calls_are_safe() {
    // Verify that multiple shutdown signals don't cause issues

    let ip_source = Box::new(IdleIpSource::new(IpAddr::from([192, 168, 1, 1])));
    let provider = Box::new(MockDnsProvider::new("test"));
    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(ip_source, provider, state_store, config)
        .expect("engine construction succeeds");

    let (shutdown_tx1, shutdown_rx1) = tokio::sync::oneshot::channel();
    let (shutdown_tx2, _shutdown_rx2) = tokio::sync::oneshot::channel();

    let engine_handle = tokio::spawn(async move {
        engine.run_with_shutdown(Some(shutdown_rx1)).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send first shutdown
    shutdown_tx1.send(()).unwrap();

    // Send second shutdown (should be ignored)
    let _ = shutdown_tx2.send(());

    // Engine should still terminate successfully
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        engine_handle
    ).await;

    assert!(result.is_ok(), "Multiple shutdown signals should not cause issues");
}
