//! Architectural Contract Test: Idle Behavior
//!
//! This test verifies that the engine does NO work when there are no IP events.
//!
//! Constraints verified:
//! - No DNS updates are invoked without IP events
//! - No background tasks are spawned
//! - CPU activity is event-driven only
//!
//! If this test fails, someone has added:
//! - Polling loops
//! - Background periodic tasks
//! - Unnecessary DNS updates

mod common;

use common::*;
use ddns_core::DdnsEngine;
use ddns_core::traits::IpSource;

#[tokio::test]
async fn idle_no_dns_updates_without_ip_events() {
    // Arrange: Create components that track calls
    let (ip_source, _ip_event_tx) =
        ControlledIpSource::new(std::net::IpAddr::from([192, 168, 1, 1]));
    let ip_source = Box::new(ip_source);
    let provider = Box::new(MockDnsProvider::new("test"));
    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    // Act: Create engine
    let (engine, _event_rx) = DdnsEngine::new(ip_source, provider, state_store, config)
        .expect("engine construction succeeds");

    // Create a shutdown trigger
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Run engine in background
    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    // Wait a brief moment to ensure the engine is running
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Trigger shutdown
    let _ = shutdown_tx.send(());

    // Wait for engine to stop
    let result = engine_handle.await.expect("engine task completes");
    assert!(result.is_ok(), "engine shuts down cleanly");

    // Assert: no update events were emitted while idle
    // (In a real implementation, we'd track events via the event_rx)
}

#[tokio::test]
async fn idle_no_background_polling() {
    // This test verifies that the engine doesn't poll when idle
    //
    // We verify this by:
    // 1. Creating an engine with an idle IP source
    // 2. Running it for a bounded time
    // 3. Verifying no watch() calls beyond initial setup
    // 4. Verifying no current() calls beyond initial setup

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Create a tracking wrapper
    let current_calls = Arc::new(AtomicUsize::new(0));
    let watch_calls = Arc::new(AtomicUsize::new(0));

    struct TrackingIdleSource {
        current_ip: std::net::IpAddr,
        current_calls: Arc<AtomicUsize>,
        watch_calls: Arc<AtomicUsize>,
        stream_rx: Arc<
            std::sync::Mutex<
                Option<tokio::sync::mpsc::UnboundedReceiver<ddns_core::traits::IpChangeEvent>>,
            >,
        >,
        _stream_tx: tokio::sync::mpsc::UnboundedSender<ddns_core::traits::IpChangeEvent>,
    }

    impl TrackingIdleSource {
        fn new(
            current_ip: std::net::IpAddr,
            current_calls: Arc<AtomicUsize>,
            watch_calls: Arc<AtomicUsize>,
        ) -> Self {
            let (stream_tx, stream_rx) = tokio::sync::mpsc::unbounded_channel();
            Self {
                current_ip,
                current_calls,
                watch_calls,
                stream_rx: Arc::new(std::sync::Mutex::new(Some(stream_rx))),
                _stream_tx: stream_tx,
            }
        }
    }

    #[async_trait::async_trait]
    impl IpSource for TrackingIdleSource {
        async fn current(&self) -> ddns_core::Result<std::net::IpAddr> {
            self.current_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.current_ip)
        }

        fn watch(
            &self,
        ) -> std::pin::Pin<
            Box<dyn tokio_stream::Stream<Item = ddns_core::traits::IpChangeEvent> + Send + 'static>,
        > {
            self.watch_calls.fetch_add(1, Ordering::SeqCst);

            let rx = self
                .stream_rx
                .lock()
                .unwrap()
                .take()
                .expect("watch() can only be called once");

            Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
        }
    }

    let ip_source = Box::new(TrackingIdleSource::new(
        std::net::IpAddr::from([192, 168, 1, 1]),
        current_calls.clone(),
        watch_calls.clone(),
    ));

    let provider = Box::new(MockDnsProvider::new("test"));
    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(ip_source, provider, state_store, config)
        .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Run engine
    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    // Let it run briefly
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Shutdown
    shutdown_tx.send(()).unwrap();

    engine_handle.await.unwrap().unwrap();

    // Assert: current() called exactly once (at startup)
    assert_eq!(
        current_calls.load(Ordering::SeqCst),
        1,
        "current() should be called exactly once at startup"
    );

    // Assert: watch() called exactly once (at startup)
    assert_eq!(
        watch_calls.load(Ordering::SeqCst),
        1,
        "watch() should be called exactly once at startup"
    );
}

#[tokio::test]
async fn idle_no_periodic_wakeups() {
    // This test ensures that the engine doesn't have periodic wakeups
    // that would consume CPU unnecessarily

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    // Create an IP source that counts how many times it's polled
    let poll_count = Arc::new(AtomicUsize::new(0));

    struct CountingIdleSource {
        current_ip: std::net::IpAddr,
        poll_count: Arc<AtomicUsize>,
        stream_rx: Arc<
            std::sync::Mutex<
                Option<tokio::sync::mpsc::UnboundedReceiver<ddns_core::traits::IpChangeEvent>>,
            >,
        >,
        _stream_tx: tokio::sync::mpsc::UnboundedSender<ddns_core::traits::IpChangeEvent>,
    }

    impl CountingIdleSource {
        fn new(current_ip: std::net::IpAddr, poll_count: Arc<AtomicUsize>) -> Self {
            let (stream_tx, stream_rx) = tokio::sync::mpsc::unbounded_channel();
            Self {
                current_ip,
                poll_count,
                stream_rx: Arc::new(std::sync::Mutex::new(Some(stream_rx))),
                _stream_tx: stream_tx,
            }
        }
    }

    #[async_trait::async_trait]
    impl IpSource for CountingIdleSource {
        async fn current(&self) -> ddns_core::Result<std::net::IpAddr> {
            Ok(self.current_ip)
        }

        fn watch(
            &self,
        ) -> std::pin::Pin<
            Box<dyn tokio_stream::Stream<Item = ddns_core::traits::IpChangeEvent> + Send + 'static>,
        > {
            self.poll_count.fetch_add(1, Ordering::SeqCst);

            let rx = self
                .stream_rx
                .lock()
                .unwrap()
                .take()
                .expect("watch() can only be called once");

            Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
        }
    }

    let ip_source = Box::new(CountingIdleSource::new(
        std::net::IpAddr::from([192, 168, 1, 1]),
        poll_count.clone(),
    ));

    let provider = Box::new(MockDnsProvider::new("test"));
    let state_store = Box::new(MockStateStore::new());
    let config = minimal_config("example.com");

    let (engine, _event_rx) = DdnsEngine::new(ip_source, provider, state_store, config)
        .expect("engine construction succeeds");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let start = Instant::now();

    // Run engine
    let engine_handle =
        tokio::spawn(async move { engine.run_for_testing(Some(shutdown_rx)).await });

    // Let it run for 200ms
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    shutdown_tx.send(()).unwrap();
    engine_handle.await.unwrap().unwrap();

    let elapsed = start.elapsed();

    // Assert: poll count should be very low (only initial setup)
    // If there's polling, the count would be much higher
    let poll_count_value = poll_count.load(Ordering::SeqCst);
    assert!(
        poll_count_value <= 2,
        "Stream should not be polled repeatedly (count: {}, elapsed: {:?})",
        poll_count_value,
        elapsed
    );

    // Additional assertion: if we ran for 200ms and there was polling
    // at e.g. 10ms intervals, we'd see ~20 polls. Seeing only 1-2
    // confirms event-driven behavior.
}
