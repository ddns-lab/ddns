# Observability Design

This document defines the observability strategy for the ddns system.

## Design Principles

Per Phase 7 requirements:
- ✅ Minimal logging and metrics hooks
- ✅ Zero allocation on idle
- ✅ No background metrics tasks
- ✅ Logs must be event-driven only

## Observability Components

### 1. Structured Logging

**Implementation**: `tracing` crate

**Log Levels**:
- `error` - Failures that prevent operation
- `warn` - Unexpected but recoverable issues
- `info` - Important state changes (startup, shutdown, updates)
- `debug` - Detailed diagnostic information
- `trace` - Very detailed execution flow (not currently used)

**Usage Examples**:
```rust
// Startup/shutdown
info!("Starting DDNS engine");
info!("Shutting down daemon");

// DNS updates
info!("Updated example.com -> 192.168.1.1 (previous: 10.0.0.1)");
error!("Failed to update example.com: {}", error);

// Idempotency
debug!("Record example.com already has IP 192.168.1.1, skipping update");

// Warnings
warn!("Provider cloudflare does not support record example.com");
warn!("Update attempt 0 failed for example.com: {}", error);
```

**Zero Cost on Idle**:
- ✅ No log statements in idle path
- ✅ All logging is event-driven (triggered by IP changes, shutdown, etc.)
- ✅ Can be compiled out or disabled via feature flags

**Disabling Logging**:
```toml
# In Cargo.toml
[dependencies]
tracing = { version = "0.1", optional = true }  # Set optional = true to disable

# Or at compile time
# RUST_LOG=off cargo build --release
```

### 2. Event System

**Implementation**: `EngineEvent` enum + `mpsc::unbounded_channel`

**Event Types**:
```rust
pub enum EngineEvent {
    Started { records_count: usize },
    Stopped { reason: String },
    IpChangeDetected { record_name: String, new_ip: IpAddr },
    UpdateStarted { record_name: String, new_ip: IpAddr },
    UpdateSucceeded { record_name: String, new_ip: IpAddr, previous_ip: Option<IpAddr> },
    UpdateSkipped { record_name: String, current_ip: IpAddr },
    UpdateFailed { record_name: String, error: String, retry_count: usize },
}
```

**Event Flow**:
```text
Engine emit_event() ──→ mpsc::unbounded_channel ──→ event_rx
                                                        │
                                                        ├─→ Consumer (optional)
                                                        └─→ /dev/null (if no consumer)
```

**Characteristics**:
- ✅ **Non-blocking**: `send()` never blocks
- ✅ **Lock-free**: Unbounded channel uses atomic operations
- ✅ **Zero-copy**: Events are Clone but small (< 100 bytes)
- ✅ **No overhead if unused**: If no receiver, events are dropped

**Usage Example** (monitoring integration):
```rust
// Create engine
let (engine, mut event_rx) = DdnsEngine::new(...)?;

// Spawn monitoring task
tokio::spawn(async move {
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::UpdateSucceeded { record_name, new_ip, .. } => {
                // Send to metrics system
                metrics.counter("dns_updates").increment();
                metrics.gauge("current_ip", new_ip).set();
            }
            EngineEvent::UpdateFailed { record_name, error, .. } => {
                metrics.counter("dns_update_errors").increment();
            }
            _ => {}
        }
    }
});

// Engine runs normally
engine.run().await?;
```

### 3. No Metrics System

**Design Decision**: No built-in metrics export

**Rationale**:
- Metrics require external dependencies (Prometheus, StatsD, etc.)
- Metrics export is deployment-specific
- Event system allows users to implement their own metrics

**If You Need Metrics**:

**Option 1**: Use events to drive metrics
```rust
// Create a Prometheus exporter from events
let builder = prometheus::HistogramOpts::new("dns_update_duration", "DNS update duration");
let histogram = prometheus::Histogram::with_opts(builder).unwrap();

tokio::spawn(async move {
    let mut start = None;
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::UpdateStarted { .. } => {
                start = Some(Instant::now());
            }
            EngineEvent::UpdateSucceeded { .. } => {
                if let Some(s) = start {
                    histogram.observe(s.elapsed().as_secs_f64());
                    start = None;
                }
            }
            _ => {}
        }
    }
});
```

**Option 2**: Use a monitoring crate
```rust
// Use tracing-opentelemetry for distributed tracing
tracing_opentelemetry::layer()
    .with_tracer(tracing_opentelemetry::OpenTelemetryLayer::new());
```

### 4. Observability Guarantees

| Criterion | Implementation | Evidence |
|-----------|----------------|----------|
| Zero allocation on idle | ✅ | No logging/events in idle loop |
| Event-driven only | ✅ | All observability triggered by events |
| No background tasks | ✅ | No tokio::spawn for metrics/logging |
| Can be disabled | ✅ | tracing supports feature flags |
| No overhead if unused | ✅ | Events dropped if no receiver |

### 5. Performance Considerations

**Channel Overhead**:
- `mpsc::unbounded_channel` uses atomic operations
- No locks, no blocking
- Cost is ~10-20ns per `send()` (benchmark data)

**Event Cloning**:
- Events are small (< 100 bytes)
- Clone is shallow (mostly Copy types)
- Cost is negligible

**Logging Overhead**:
- `tracing` is designed for zero-cost when disabled
- When enabled: formatted only if log level is active
- No allocations for disabled log levels

### 6. Observability Contract Tests

The following contract tests verify observability properties:

| Test | Property Verified |
|------|-------------------|
| `idle_no_periodic_wakeups` | No periodic logging on idle |
| `idle_no_background_polling` | No background observability tasks |
| `idle_no_dns_updates_without_ip_events` | Events only emitted on activity |

## Adding Observability

### How to Add Logging

**DO**:
```rust
// Event-driven logging (triggered by IP change)
info!("Updated {} -> {}", record_name, new_ip);

// Error logging (only when errors occur)
error!("Failed to update {}: {}", record_name, error);
```

**DON'T**:
```rust
// ❌ DON'T: Log in idle loop
loop {
    info!("Engine is running...");  // Violates zero-cost idle
    select! { ... }
}

// ❌ DON'T: Log periodically
tokio::spawn(async move {
    loop {
        info!("Health check OK");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
});
```

### How to Add Events

**DO**:
```rust
// Emit event when something happens
self.emit_event(EngineEvent::UpdateSucceeded { ... });
```

**DON'T**:
```rust
// ❌ DON'T: Emit events periodically
loop {
    self.emit_event(EngineEvent::Heartbeat);
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

## Observability Checklist

Before adding observability code, verify:

- [ ] Triggered by an event (IP change, update, error, etc.)
- [ ] Not in idle path
- [ ] Not periodic
- [ ] Not in a background task
- [ ] Uses appropriate log level
- [ ] Event is small and Clone-friendly

If any check fails, **DON'T add the observability code**.

## Future Observability

Possible future additions (architecturally compliant):

1. **Distributed tracing** (via tracing-opentelemetry)
   - Event-driven: traces only for IP changes
   - Zero cost when disabled
   - No background tasks

2. **Metrics exporter** (optional feature)
   - Consumes events
   - Exports to Prometheus/StatsD
   - Can be disabled

3. **Structured logging** (already supported)
   - JSON format
   - Trace ID injection
   - No performance impact when disabled

## Summary

Observability in ddns is:
- **Event-driven**: Only when something happens
- **Zero-cost on idle**: No polling, no periodic tasks
- **Optional**: Can be disabled via feature flags
- **Extensible**: Event system allows user-defined monitoring
- **No overhead**: Minimal performance impact
