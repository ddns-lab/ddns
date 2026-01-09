# LIFECYCLE.md

This document defines the **lifecycle semantics** for the `ddns` system.

It specifies:
- Startup sequence and invariants
- Ownership and boundaries of async tasks
- Shutdown behavior (graceful and forced)
- Cancellation guarantees
- Resource cleanup requirements

This document is authoritative. Implementation MUST follow these semantics.

---

## 1. Scope

This document covers the lifecycle of:

1. **`ddnsd` (daemon)**: Process startup and shutdown
2. **`DdnsEngine`**: Core engine runtime
3. **Plugin components**: `IpSource`, `DnsProvider`, `StateStore`

### 1.1 Out of Scope

- Operating system process lifecycle (systemd, init, containers)
- Child process spawning (not used in this architecture)
- Hot-reload or live reconfiguration (explicitly prohibited by AI_CONTRACT.md §6)

---

## 2. Daemon Lifecycle (`ddnsd`)

### 2.1 Startup Sequence

The daemon MUST execute startup in the following order:

```
1. Parse environment variables
2. Initialize tracing (logging)
3. Validate configuration
   - If invalid: log fatal error, exit with status 1
4. Create ProviderRegistry
5. Register built-in providers (if feature flags enabled)
6. Construct IpSource from config
   - If invalid: log fatal error, exit with status 1
7. Construct DnsProvider from config
   - If invalid: log fatal error, exit with status 1
8. Construct StateStore from config
   - If unavailable or corrupted: log fatal error, exit with status 1
9. Load state from StateStore (if any)
10. Construct DdnsEngine
11. Enter tokio runtime
12. Spawn engine task (owned by runtime)
13. Wait for shutdown signal (SIGTERM or SIGINT)
14. Initiate graceful shutdown
15. Exit with status 0
```

**Requirements**:
- Each step MUST succeed before proceeding
- Fatal errors MUST log details before exit
- Exit status codes:
  - `0`: Clean shutdown
  - `1`: Configuration or startup error
  - `2`: Runtime error (unexpected)

**Prohibited**:
- Daemon MUST NOT start without valid configuration
- Daemon MUST NOT start with unavailable StateStore
- Daemon MUST NOT skip startup validation for "convenience"

---

### 2.2 Shutdown Triggers

Shutdown is initiated by:

1. **SIGTERM** (preferred): Request graceful shutdown
2. **SIGINT** (Ctrl+C): Request graceful shutdown
3. **IpSource stream termination**: Implicit graceful shutdown (see FAILURE_MODEL.md §2.1.1)

**Requirements**:
- Daemon MUST handle both SIGTERM and SIGINT
- Daemon MUST NOT differentiate between signals (same shutdown path)
- Signal handler MUST initiate graceful shutdown, not terminate immediately

---

### 2.3 Graceful Shutdown Sequence

When shutdown is triggered:

```
1. Signal handler sets cancellation token
2. Engine event loop receives cancellation
3. Engine breaks from event loop
4. Engine calls StateStore::flush()
   - MUST complete with timeout (e.g., 5 seconds)
   - If timeout: log error, proceed with shutdown
5. Engine drops IpSource stream (cancels watch task)
6. Engine drops DnsProvider and StateStore
7. Engine task completes
8. Tokio runtime shuts down
9. Main function returns
10. Process exits
```

**Requirements**:
- Step 4 (StateStore flush) MUST be attempted
- Flush MUST have a timeout (prevents indefinite shutdown)
- Drop order MUST reverse initialization order (RAII semantics)

**Timeout values**:
- StateStore flush: **5 seconds**
- Outstanding DNS updates: **10 seconds** (canceled if not complete)
- Total shutdown: **30 seconds** maximum (after this, forced termination)

**Prohibited**:
- Shutdown MUST NOT block indefinitely on any component
- Shutdown MUST NOT skip StateStore flush without timeout attempt
- Shutdown MUST NOT leave spawned tasks orphaned

---

### 2.4 Forced Termination

If graceful shutdown does not complete within **30 seconds**:

```
1. Daemon logs "shutdown timeout, forcing termination"
2. Tokio runtime is shutdown (if still running)
3. Process exits via std::process::exit(1)
```

**Requirements**:
- Forced termination MUST be logged
- Exit status MUST be non-zero
- This is an exceptional case (indicates bug or resource exhaustion)

**Rationale**: Prevents "zombie" processes that never exit.

---

## 3. Engine Lifecycle (`DdnsEngine`)

### 3.1 Engine Initialization

**Requirements**:
- Engine MUST be constructed BEFORE entering the runtime
- Engine MUST validate configuration during construction
- Engine MUST NOT perform I/O during construction

**Construction contract**:

```rust
pub fn new(
    ip_source: Box<dyn IpSource>,
    provider: Box<dyn DnsProvider>,
    state_store: Box<dyn StateStore>,
    config: DdnsConfig,
) -> Result<(Self, mpsc::UnboundedReceiver<EngineEvent>)>
```

**Invariants after construction**:
- All components are owned by Engine
- Event receiver is returned to caller
- No async tasks are spawned yet

---

### 3.2 Engine Runtime

**Entry point**: `DdnsEngine::run(&self) -> Result<()>`

**Requirements**:
- `run()` MUST be called from within a tokio runtime
- `run()` MUST spawn no tasks except the IpSource watch task
- `run()` MUST run on the current task (not spawn a separate engine task)

**Event loop structure**:

```rust
pub async fn run(&self) -> Result<()> {
    // 1. Emit EngineEvent::Started
    // 2. Get initial IP from IpSource
    // 3. Enter event loop:
    //    select! {
    //        Some(event) = ip_stream.next() => {
    //            handle_ip_change(event).await;
    //        }
    //        _ = shutdown_signal.recv() => {
    //            break; // Exit loop
    //        }
    //    }
    // 4. StateStore::flush().await
    // 5. Emit EngineEvent::Stopped
    // 6. Ok(())
}
```

**Requirements**:
- Event loop MUST respond to shutdown signal
- Event loop MUST process IP changes to completion before checking shutdown
- Event loop MUST NOT have a timeout (runs until signal)

---

### 3.3 Async Task Ownership

**Owned tasks**:

| Task | Owner | Lifetime |
|------|-------|----------|
| IpSource::watch() stream | Engine event loop | Until shutdown |
| DNS update tasks | DnsProvider (internal) | Scoped to update operation |
| StateStore I/O | StateStore (internal) | Scoped to operation |

**Prohibited**:
- Engine MUST NOT spawn detached tasks (`tokio::spawn` without handle)
- Engine MUST NOT spawn tasks that outlive the Engine
- IpSource MUST NOT spawn background tasks beyond the watch stream

**Cancellation**:
- Dropping the IpSource stream MUST cancel the watch task
- Engine MUST NOT explicitly cancel tasks (drop semantics only)

---

### 3.4 Shutdown Semantics

**Normal shutdown**:

```
1. Shutdown signal received
2. Event loop breaks from select!
3. Engine calls StateStore::flush().await
   - Timeout: 5 seconds
   - Error logged if timeout or failure
4. Engine emits EngineEvent::Stopped
5. Engine returns Ok(())
6. Engine is dropped (RAII cleanup)
```

**IpSource stream termination shutdown**:

```
1. IpSource::watch() returns None (error)
2. Event loop breaks
3. Same as normal shutdown (steps 3-6)
```

**Requirements**:
- Shutdown MUST be idempotent (safe to call multiple times)
- Shutdown MUST complete even if some components fail
- StateStore flush MUST be attempted even if IpSource failed

---

### 3.5 Invariants During Runtime

**Invariant 1**: Engine processes events sequentially
- No concurrent IP change handling
- DNS updates for Record A and Record B are serialized

**Invariant 2**: Engine owns all components
- IpSource, DnsProvider, StateStore are NOT shared
- No external references to Engine internals

**Invariant 3**: Engine never panics
- All errors are returned, not unwrapped
- Panics are bugs, not error handling

---

## 4. Plugin Lifecycle

### 4.1 IpSource Lifecycle

**Creation**:
- Created by `IpSourceFactory::create()`
- Returned as `Box<dyn IpSource>`
- No I/O during construction

**Active phase**:
- `current()` called to get initial IP
- `watch()` called to start monitoring
- Watch stream is polled by Engine

**Termination**:
- Engine drops the stream
- IpSource MUST clean up resources
- IpSource MUST NOT block indefinitely on drop

**Requirements**:
- `watch()` MUST return `Pin<Box<dyn Stream>>`
- Stream MUST be cancellation-safe
- Drop MUST terminate within **1 second**

**Prohibited**:
- IpSource MUST NOT spawn detached tasks
- IpSource MUST NOT continue running after drop
- IpSource MUST NOT require explicit shutdown call

---

### 4.2 DnsProvider Lifecycle

**Creation**:
- Created by `DnsProviderFactory::create()`
- Returned as `Box<dyn DnsProvider>`
- No I/O during construction

**Active phase**:
- `update_record()` called by Engine
- `get_record()` called by Engine (optional)
- Each call is independent

**Termination**:
- Engine drops the provider
- Provider MUST clean up resources
- Provider MUST cancel in-flight requests

**Requirements**:
- `update_record()` MUST complete or return Error
- Timeouts are Provider's responsibility (or Engine's via tokio::time::timeout)
- Drop MUST terminate within **2 seconds**

**Prohibited**:
- Provider MUST NOT spawn background tasks
- Provider MUST NOT continue API calls after drop
- Provider MUST NOT cache state across updates (unless documented)

---

### 4.3 StateStore Lifecycle

**Creation**:
- Created by `StateStoreFactory::create()`
- Returned as `Box<dyn StateStore>`
- Constructor MUST validate backend availability
- Constructor MUST fail-fast if unavailable

**Active phase**:
- `get_last_ip()` called by Engine
- `set_last_ip()` called by Engine
- `flush()` called by Engine on shutdown

**Termination**:
- Engine calls `flush()` before drop
- Engine drops the StateStore
- StateStore MUST close file handles, connections, etc.

**Requirements**:
- `flush()` MUST persist all pending writes
- `flush()` MUST have a timeout (e.g., 5 seconds)
- Drop MUST close resources within **1 second**

**Prohibited**:
- StateStore MUST NOT buffer writes indefinitely
- StateStore MUST NOT spawn background flush tasks
- StateStore MUST NOT lose data on drop

---

## 5. Cancellation Guarantees

### 5.1 Cancellation Safety

**Requirement**: All async operations MUST be cancellation-safe.

**Definition**: Cancellation is safe if:
- No resources are leaked
- No inconsistent state is observable
- Drop completes in bounded time

**Components**:

| Component | Cancellation Safety |
|-----------|---------------------|
| IpSource::watch() stream | MUST cancel on drop, cleanup within 1s |
| DnsProvider::update_record() | MAY cancel mid-flight, but SHOULD complete in-flight request |
| StateStore operations | MUST complete or timeout, no cancellation |

**Requirements**:
- Dropping the IpSource stream MUST terminate the watch task
- DnsProvider SHOULD allow in-flight requests to complete (best-effort)
- StateStore operations MUST NOT be cancelled (use timeout instead)

---

### 5.2 Timeout Guarantees

**Mandatory timeouts**:

| Operation | Timeout | Behavior on Timeout |
|-----------|---------|---------------------|
| DnsProvider::update_record() | 30s (configurable) | Return Error::Timeout |
| StateStore::flush() | 5s | Log error, return Ok() |
| Shutdown (total) | 30s | Force termination |

**Requirements**:
- Timeouts MUST be implemented via `tokio::time::timeout`
- Timeout MUST be logged at WARN level
- Timeout MUST NOT trigger panic

---

### 5.3 Bounded Operations

All operations MUST complete in bounded time:

| Operation | Maximum Duration |
|-----------|------------------|
| IpSource::current() | 5 seconds |
| IpSource::watch() next event | Unbounded (event-driven) |
| DnsProvider::update_record() | 30 seconds (with timeout) |
| StateStore read/write | 1 second |
| StateStore::flush() | 5 seconds |

**Requirements**:
- Unbounded operations MUST be cancellable
- Bounded operations MUST return Error if timeout exceeded

---

## 6. Resource Cleanup

### 6.1 Cleanup Order

On shutdown (during drop):

```
1. Cancel IpSource stream (drop)
2. Drop DnsProvider
3. Call StateStore::flush() (with timeout)
4. Drop StateStore
5. Drop Engine
```

**Requirements**:
- Order MUST be deterministic
- Each step MUST complete even if previous step failed
- Failures during cleanup MUST be logged, not panic

---

### 6.2 RAII Semantics

**Requirement**: All cleanup happens via Drop.

**Prohibited**:
- Explicit `close()` methods
- Explicit `shutdown()` methods
- Two-phase shutdown (unless explicitly documented)

**Rationale**: RAII prevents resource leaks via compiler enforcement.

**Exception**: `StateStore::flush()` is explicitly called before drop to ensure durability.

---

### 6.3 Leak Prevention

**Requirements**:
- No file descriptors left open
- No network connections left open
- No tasks left running
- no memory leaked (obviously)

**Verification**:
- All types implement Drop
- Drop implementations close resources
- No detached tasks spawned

---

## 7. Error Handling During Lifecycle

### 7.1 Startup Errors

**Behavior**: Fail-fast, exit with status 1.

**Requirements**:
- Log error details
- Do not attempt recovery
- Exit immediately

---

### 7.2 Runtime Errors

**Behavior**: Log and continue (if possible).

**Requirements**:
- Transient errors are logged and retried
- Fatal errors cause graceful shutdown
- Panics are bugs (not handled)

---

### 7.3 Shutdown Errors

**Behavior**: Log and proceed with shutdown.

**Requirements**:
- Errors during StateStore flush are logged
- Errors during drop are logged (if possible)
- Shutdown MUST NOT fail due to cleanup errors

---

## 8. Testing Requirements

Implementation MUST include tests for:

- [ ] Startup with invalid configuration (fails)
- [ ] Startup with unavailable StateStore (fails)
- [ ] Graceful shutdown completes within 30s
- [ ] Forced termination occurs after 30s timeout
- [ ] IpSource stream drop cancels task
- [ ] DnsProvider drop cancels in-flight requests
- [ ] StateStore flush persists state
- [ ] No task leaks after shutdown
- [ ] No resource leaks after drop
- [ ] Multiple shutdown calls are idempotent

---

## 9. Evolution Rules

Changes to this document:

- MUST NOT violate RAII principles
- MUST NOT introduce two-phase shutdown without justification
- MUST NOT increase shutdown timeout beyond 30s
- MUST NOT remove cancellation safety guarantees
- MUST update tests accordingly

---

## 10. Summary

The lifecycle model is characterized by:

- **Explicit startup**: All validation happens before runtime
- **Event-driven runtime**: No periodic tasks, no background loops
- **RAII cleanup**: Resources cleaned up via Drop
- **Bounded shutdown**: All timeouts enforced, forced termination as safety net
- **No task leaks**: Cancellation-safe by design

This ensures:
- Predictable behavior
- No resource leaks
- Clean shutdown
- Debuggable failures

---

This document is part of the authoritative architecture.
See: ARCHITECTURE.md, PERFORMANCE.md, FAILURE_MODEL.md, .ai/AI_CONTRACT.md
