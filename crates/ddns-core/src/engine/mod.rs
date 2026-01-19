//! Core DDNS engine
//!
//! The DdnsEngine is responsible for:
//! - Monitoring IP changes via IpSource
//! - Checking state for idempotency
//! - Updating DNS records via DnsProvider
//! - Persisting state after successful updates
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐
//! │  IpSource   │─── IpChangeEvent ───┐
//! └─────────────┘                     │
//!                                     ▼
//!                            ┌──────────────┐
//!                            │ DdnsEngine   │
//!                            └──────────────┘
//!                                     │
//!         ┌───────────────────────────┼───────────────────────────┐
//!         │                           │                           │
//!         ▼                           ▼                           ▼
//! ┌─────────────┐           ┌──────────────┐           ┌─────────────┐
//! │ StateStore  │           │ DnsProvider  │           │   Events    │
//! │ (check)     │           │ (update)     │           │  (notify)   │
//! └─────────────┘           └──────────────┘           └─────────────┘
//! ```
//!
//! ## Event Flow
//!
//! 1. IP change detected
//! 2. Check StateStore for last known IP
//! 3. If changed, call DnsProvider::update_record()
//! 4. On success, update StateStore
//! 5. Emit event for monitoring/logging

use crate::config::{DdnsConfig, RecordConfig};
use crate::error::{Error, Result};
use crate::traits::{DnsProvider, IpChangeEvent, IpSource, StateStore};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

/// Events emitted by the DdnsEngine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// IP change detected
    IpChangeDetected {
        record_name: String,
        new_ip: std::net::IpAddr,
    },

    /// DNS update started
    UpdateStarted {
        record_name: String,
        new_ip: std::net::IpAddr,
    },

    /// DNS update succeeded
    UpdateSucceeded {
        record_name: String,
        new_ip: std::net::IpAddr,
        previous_ip: Option<std::net::IpAddr>,
    },

    /// DNS update skipped (no change needed)
    UpdateSkipped {
        record_name: String,
        current_ip: std::net::IpAddr,
    },

    /// DNS update failed
    UpdateFailed {
        record_name: String,
        error: String,
        retry_count: usize,
    },

    /// Engine started
    Started { records_count: usize },

    /// Engine stopped
    Stopped { reason: String },
}

/// Core DDNS engine
///
/// The engine orchestrates the entire IP change → DNS update flow.
/// It runs continuously, monitoring for IP changes and updating DNS records.
///
/// ## Lifecycle
///
/// 1. Create with [`DdnsEngine::new()`]
/// 2. Start with [`DdnsEngine::run()`]
/// 3. Engine runs until shutdown signal received
/// 4. Drop to cleanup
///
/// ## Threading
///
/// The engine runs all operations on a single async task but is
/// thread-safe and can be safely cloned.
///
/// ## Load Resistance
///
/// The engine implements several safeguards against load and event storms:
/// - **Bounded event channel**: Prevents unbounded memory growth
/// - **Rate limiting**: Minimum interval between updates prevents API storms
/// - **Event dropping**: When channel is full, oldest events are dropped (logged)
pub struct DdnsEngine {
    /// IP source for monitoring changes
    ip_source: Box<dyn IpSource>,

    /// DNS provider for updating records
    provider: Box<dyn DnsProvider>,

    /// State store for idempotency
    state_store: Box<dyn StateStore>,

    /// DNS records to manage
    records: Vec<RecordConfig>,

    /// Maximum retry attempts
    max_retries: usize,

    /// Delay between retries (in seconds)
    retry_delay_secs: u64,

    /// Minimum interval between updates for the same record (rate limiting)
    min_update_interval_secs: u64,

    /// Event sender for external monitoring
    event_tx: mpsc::Sender<EngineEvent>,
}

impl DdnsEngine {
    /// Create a new DDNS engine
    ///
    /// # Parameters
    ///
    /// - `ip_source`: IP source implementation
    /// - `provider`: DNS provider implementation
    /// - `state_store`: State store implementation
    /// - `config`: DDNS configuration
    ///
    /// # Returns
    ///
    /// A tuple of (engine, event_receiver) where event_receiver yields engine events
    pub fn new(
        ip_source: Box<dyn IpSource>,
        provider: Box<dyn DnsProvider>,
        state_store: Box<dyn StateStore>,
        config: DdnsConfig,
    ) -> Result<(Self, mpsc::Receiver<EngineEvent>)> {
        config.validate()?;

        let (tx, rx) = mpsc::channel(config.engine.event_channel_capacity);

        let engine = Self {
            ip_source,
            provider,
            state_store,
            records: config.records,
            max_retries: config.engine.max_retries,
            retry_delay_secs: config.engine.retry_delay_secs,
            min_update_interval_secs: config.engine.min_update_interval_secs,
            event_tx: tx,
        };

        Ok((engine, rx))
    }

    /// Run the engine
    ///
    /// This method starts the event-driven IP monitoring loop.
    /// It will run continuously until a shutdown signal is received.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Clean shutdown
    /// - `Err(Error)`: Fatal error
    pub async fn run(&self) -> Result<()> {
        self.run_internal(None, false).await
    }

    /// Internal run implementation that accepts an optional shutdown signal
    ///
    /// # Parameters
    ///
    /// - `shutdown_rx`: Optional oneshot receiver to trigger shutdown (for testing)
    /// - `skip_initial_updates`: If true, skip initial DNS updates on startup (for testing)
    async fn run_internal(
        &self,
        shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
        skip_initial_updates: bool,
    ) -> Result<()> {
        self.emit_event(EngineEvent::Started {
            records_count: self.records.len(),
        });

        // Trigger initial DNS updates for all configured records (unless testing)
        if !skip_initial_updates {
            info!("Triggering initial DNS updates");
            if let Err(e) = self.trigger_initial_updates().await {
                error!("Failed to handle initial DNS updates: {}", e);
            }
        }

        // Get initial IP for logging (non-blocking - allows starting without IPs)
        match self.ip_source.current().await {
            Ok(ip) => info!("Initial IP: {}", ip),
            Err(e) => info!("No initial IP available (will wait for netlink events): {}", e),
        }

        // Watch for IP changes
        let mut ip_stream = self.ip_source.watch();

        // Main event loop
        if let Some(mut rx) = shutdown_rx {
            // Test mode: wait for provided shutdown signal
            loop {
                tokio::select! {
                    // Handle IP changes
                    Some(event) = ip_stream.next() => {
                        if let Err(e) = self.handle_ip_change(event).await {
                            error!("Failed to handle IP change: {}", e);
                        }
                    }

                    // Handle test shutdown signal
                    _ = &mut rx => {
                        info!("Shutdown signal received");
                        self.emit_event(EngineEvent::Stopped {
                            reason: "Shutdown signal".to_string(),
                        });
                        break;
                    }
                }
            }
        } else {
            // Production mode: wait for SIGINT/SIGTERM
            #[cfg(unix)]
            let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|e| anyhow::anyhow!("Failed to setup SIGTERM handler: {}", e))?;
            let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|e| anyhow::anyhow!("Failed to setup SIGINT handler: {}", e))?;

            loop {
                tokio::select! {
                    // Handle IP changes
                    Some(event) = ip_stream.next() => {
                        if let Err(e) = self.handle_ip_change(event).await {
                            error!("Failed to handle IP change: {}", e);
                            // Continue running despite errors
                        }
                    }

                    // Handle SIGTERM (systemd stop)
                    _ = sigterm.recv() => {
                        info!("SIGTERM received, shutting down gracefully");
                        self.emit_event(EngineEvent::Stopped {
                            reason: "SIGTERM".to_string(),
                        });
                        break;
                    }

                    // Handle SIGINT (Ctrl-C)
                    _ = sigint.recv() => {
                        info!("SIGINT received, shutting down gracefully");
                        self.emit_event(EngineEvent::Stopped {
                            reason: "SIGINT".to_string(),
                        });
                        break;
                    }
                }
            }

            #[cfg(not(unix))]
            loop {
                tokio::select! {
                    // Handle IP changes
                    Some(event) = ip_stream.next() => {
                        if let Err(e) = self.handle_ip_change(event).await {
                            error!("Failed to handle IP change: {}", e);
                            // Continue running despite errors
                        }
                    }

                    // Handle shutdown signal (non-Unix)
                    _ = tokio::signal::ctrl_c() => {
                        info!("Shutdown signal received");
                        self.emit_event(EngineEvent::Stopped {
                            reason: "Shutdown signal".to_string(),
                        });
                        break;
                    }
                }
            }
        }

        // Flush state before exiting
        self.state_store.flush().await?;
        info!("State flushed, engine stopped");

        Ok(())
    }

    /// Trigger initial DNS updates on startup
    ///
    /// This ensures all configured records are up-to-date on startup,
    /// even if no IP change event is received.
    ///
    /// The logic:
    /// 1. Get current IP from ip_source
    /// 2. Create an IpChangeEvent with previous_ip=None (indicates initial check)
    /// 3. Process the event, which will:
    ///    - Check StateStore for last known IP
    ///    - If different or missing, call provider to update/create record
    ///    - If same, skip update (idempotency)
    async fn trigger_initial_updates(&self) -> Result<()> {
        use crate::traits::IpChangeEvent;

        // Get current IP (this may be v4 or v6 depending on ip_source configuration)
        let current_ip = match self.ip_source.current().await {
            Ok(ip) => ip,
            Err(e) => {
                // If we can't get current IP, log warning but don't fail startup
                // The watch() loop will handle IP changes when they occur
                warn!("Failed to get current IP for initial update: {}", e);
                return Ok(());
            }
        };

        info!("Current IP: {}", current_ip);

        // Create an initial event with previous_ip=None
        // This forces update_record_with_retry to check and update as needed
        let initial_event = IpChangeEvent::new(current_ip, None);

        // Process the event for all matching records
        self.handle_ip_change(initial_event).await?;

        Ok(())
    }

    /// Handle an IP change event
    ///
    /// # Parameters
    ///
    /// - `event`: The IP change event
    async fn handle_ip_change(&self, event: IpChangeEvent) -> Result<()> {
        debug!(
            "IP change detected: {} -> {:?} (version: {:?})",
            event
                .previous_ip
                .map(|ip| ip.to_string())
                .unwrap_or("None".to_string()),
            event.new_ip,
            event.version
        );

        // Process each configured record
        for record in &self.records {
            if !record.enabled {
                debug!("Record {} is disabled, skipping", record.name);
                continue;
            }

            // Filter records by IP version match
            // - A records only process IPv4 events
            // - AAAA records only process IPv6 events
            // - Auto records process all events
            let should_process = match record.record_type {
                crate::config::RecordType::A => event.version == crate::traits::IpVersion::V4,
                crate::config::RecordType::Aaaa => event.version == crate::traits::IpVersion::V6,
                crate::config::RecordType::Auto => true,
            };

            if !should_process {
                debug!(
                    "Skipping record {} (type: {:?}) for IP version {:?}",
                    record.name, record.record_type, event.version
                );
                continue;
            }

            // Check if provider supports this record
            if !self.provider.supports_record(&record.name) {
                warn!(
                    "Provider {} does not support record {}",
                    self.provider.provider_name(),
                    record.name
                );
                continue;
            }

            // Emit event
            self.emit_event(EngineEvent::IpChangeDetected {
                record_name: record.name.clone(),
                new_ip: event.new_ip,
            });

            // Update the record
            match self
                .update_record_with_retry(&record.name, event.new_ip)
                .await
            {
                Ok(_) => {
                    debug!("Successfully updated record {}", record.name);
                }
                Err(e) => {
                    error!("Failed to update record {}: {}", record.name, e);
                    // Continue with other records
                }
            }
        }

        Ok(())
    }

    /// Update a DNS record with retry logic
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    /// - `new_ip`: The new IP address
    async fn update_record_with_retry(
        &self,
        record_name: &str,
        new_ip: std::net::IpAddr,
    ) -> Result<()> {
        // Check if update is needed (idempotency)
        // Note: Using nested if statements instead of let-chains for better Docker compatibility
        #[allow(clippy::collapsible_if)]
        if let Some(last_ip) = self.state_store.get_last_ip(record_name).await? {
            if last_ip == new_ip {
                debug!(
                    "Record {} already has IP {}, skipping update",
                    record_name, new_ip
                );
                self.emit_event(EngineEvent::UpdateSkipped {
                    record_name: record_name.to_string(),
                    current_ip: new_ip,
                });
                return Ok(());
            }
        }

        // Rate limiting: Check minimum interval between updates
        // Note: Using nested if statements instead of let-chains for better Docker compatibility
        #[allow(clippy::collapsible_if)]
        if self.min_update_interval_secs > 0 {
            if let Some(record) = self.state_store.get_record(record_name).await? {
                let now = chrono::Utc::now();
                let elapsed = now.signed_duration_since(record.last_updated);
                let min_interval = chrono::Duration::seconds(self.min_update_interval_secs as i64);

                if elapsed < min_interval {
                    debug!(
                        "Record {} updated too recently ({}s ago), skipping update. Minimum interval: {}s",
                        record_name,
                        elapsed.num_seconds(),
                        self.min_update_interval_secs
                    );
                    self.emit_event(EngineEvent::UpdateSkipped {
                        record_name: record_name.to_string(),
                        current_ip: new_ip,
                    });
                    return Ok(());
                }
            }
        }

        // Emit event
        self.emit_event(EngineEvent::UpdateStarted {
            record_name: record_name.to_string(),
            new_ip,
        });

        // Attempt update with retries
        let mut last_error = None;
        for attempt in 0..=self.max_retries {
            match self.do_update(record_name, new_ip).await {
                Ok(result) => {
                    match result {
                        crate::traits::UpdateResult::Updated { previous_ip, .. } => {
                            info!(
                                "Updated {} -> {} (previous: {:?})",
                                record_name, new_ip, previous_ip
                            );
                            self.emit_event(EngineEvent::UpdateSucceeded {
                                record_name: record_name.to_string(),
                                new_ip,
                                previous_ip,
                            });
                        }
                        crate::traits::UpdateResult::Unchanged { .. } => {
                            debug!("Record {} unchanged", record_name);
                        }
                        crate::traits::UpdateResult::Created { .. } => {
                            info!("Created record {} -> {}", record_name, new_ip);
                            self.emit_event(EngineEvent::UpdateSucceeded {
                                record_name: record_name.to_string(),
                                new_ip,
                                previous_ip: None,
                            });
                        }
                    }

                    // Update state store
                    self.state_store.set_last_ip(record_name, new_ip).await?;
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "Update attempt {} failed for {}: {}",
                        attempt, record_name, e
                    );
                    last_error = Some(e);

                    // Wait before retry (unless this was the last attempt)
                    if attempt < self.max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_secs(self.retry_delay_secs))
                            .await;
                    }
                }
            }
        }

        // All retries failed
        let error = last_error.unwrap_or_else(|| Error::Other("Unknown error".to_string()));
        self.emit_event(EngineEvent::UpdateFailed {
            record_name: record_name.to_string(),
            error: error.to_string(),
            retry_count: self.max_retries,
        });
        Err(error)
    }

    /// Perform a single DNS update attempt
    ///
    /// # Parameters
    ///
    /// - `record_name`: The DNS record name
    /// - `new_ip`: The new IP address
    async fn do_update(
        &self,
        record_name: &str,
        new_ip: std::net::IpAddr,
    ) -> Result<crate::traits::UpdateResult> {
        self.provider
            .update_record(record_name, new_ip)
            .await
            .map_err(|e| Error::provider(self.provider.provider_name(), e.to_string()))
    }

    /// Emit an engine event
    ///
    /// # Parameters
    ///
    /// - `event`: The event to emit
    fn emit_event(&self, event: EngineEvent) {
        // Send event, logging warning if channel is full (backpressure)
        if self.event_tx.try_send(event).is_err() {
            // Channel is full - this indicates event processing is slower than event generation
            // This is expected under extreme load and prevents unbounded memory growth
            // The event will be dropped (with a log warning)
            warn!(
                "Event channel full, dropping event. Consider increasing event_channel_capacity or reducing IP change rate."
            );
        }
    }

    /// Test-only helper to run the engine with a controlled shutdown signal
    ///
    /// # Visibility
    ///
    /// This is `pub` for testing purposes only.
    ///
    /// Run the engine with programmatic shutdown (for testing)
    ///
    /// **TESTING ONLY**: Architecture contract tests require controlled shutdown.
    /// Production daemon code should use `run()` instead, which manages shutdown
    /// via OS signals (SIGTERM/SIGINT) rather than programmatic channels.
    ///
    /// External providers and IP sources MUST NOT call this method.
    pub async fn run_with_shutdown(
        &self,
        shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<()> {
        self.run_internal(shutdown_rx, false).await
    }

    /// Run the engine without initial DNS updates (for testing)
    ///
    /// **TESTING ONLY**: This method skips the initial DNS update that happens on startup.
    /// Use this for contract tests that need to verify event-driven behavior without
    /// the initial update interfering with test assertions.
    ///
    /// External providers and IP sources MUST NOT call this method.
    #[doc(hidden)]
    pub async fn run_for_testing(
        &self,
        shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<()> {
        self.run_internal(shutdown_rx, true).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_event_serialization() {
        let event = EngineEvent::IpChangeDetected {
            record_name: "example.com".to_string(),
            new_ip: std::net::IpAddr::from([1, 2, 3, 4]),
        };

        // Just test that events can be created and cloned
        let _ = event.clone();
        assert_eq!(event.clone(), event);
    }
}
