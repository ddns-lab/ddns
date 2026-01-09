# FAILURE_MODEL.md

This document defines the **failure model** for the `ddns` system.

It specifies:
- What failures are explicitly modeled and handled
- What failures are explicitly out of scope
- Required system behavior under failure conditions

This document is authoritative. Implementation MUST follow these semantics.

---

## 1. Failure Model Scope

### 1.1 Modeled Failures

The following failure classes are **explicitly modeled**:

| Component | Failure Mode | Handling Strategy |
|-----------|--------------|-------------------|
| `IpSource` | Stream termination | Engine logs error, enters degraded mode |
| `IpSource` | Invalid event (e.g., loopback IP) | Event is discarded, logged at WARN |
| `IpSource` | Repeated failures | Engine continues attempting, no special backoff |
| `DnsProvider` | Transient network failure | Exponential backoff, bounded retry (max N attempts) |
| `DnsProvider` | Authentication failure | Immediate failure, no retry (operator intervention required) |
| `DnsProvider` | Rate limiting | Backoff per provider semantics, retry |
| `DnsProvider` | Partial update (multi-record) | Failure terminates batch, state reflects completed updates |
| `StateStore` | Unavailable at startup | Daemon MUST refuse to start (fail-fast) |
| `StateStore` | Unavailable during runtime | Engine continues in-memory, logs error, persists when available |
| `StateStore` | Corrupted state file | Daemon MUST refuse to start (fail-fast) |
| `StateStore` | Write failure | In-memory state continues, error logged |
| `Network` | Intermittent connectivity | Handled by DnsProvider retry logic |
| `Daemon` | Crash / restart | StateStore provides recovery state |

### 1.2 Out-of-Scope Failures

The following are **explicitly NOT modeled**:

- **Disk space exhaustion**: Treated as StateStore write failure
- **System clock skew**: Not handled, operator responsibility
- **Memory pressure**: Not handled, OOM is terminal
- **Provider-specific partial API failures**: Handled as provider errors
- **Malicious compromise**: Out of scope

Rationale:
- These failures either require OS-level intervention or are catastrophic.
- Adding handling would violate the "minimal complexity" principle (see ARCHITECTURE.md).

---

## 2. Component Failure Semantics

### 2.1 IpSource Failures

#### 2.1.1 Stream Termination

**Requirement**: `IpSource::watch()` MUST terminate (return `None`) only on unrecoverable error.

**Behavior**:
- Engine detects stream termination
- Engine emits `EngineEvent::Stopped { reason: "IpSource stream terminated" }`
- Engine initiates graceful shutdown

**Prohibited**:
- IpSource MUST NOT silently restart its stream
- IpSource MUST NOT spawn background tasks to recover

#### 2.1.2 Invalid Events

**Definition**: Invalid event = event that cannot be reasonably processed.

Examples:
- Loopback IP (`127.0.0.1`, `::1`)
- Unspecified address (`0.0.0.0`, `::`)
- Link-local IPv6 (`fe80::/10`) in certain contexts

**Behavior**:
- Engine MUST discard invalid events
- Engine MUST log at `WARN` level
- Engine MUST continue processing subsequent events

**Prohibited**:
- Engine MUST NOT crash on invalid events
- Engine MUST NOT enter error state

#### 2.1.3 IpSource Blocking Forever

**Definition**: Stream that never yields or terminates.

**Behavior**:
- This is an implementation bug, not a modeled failure mode.
- Operator must detect (no IP updates) and restart daemon.

---

### 2.2 DnsProvider Failures

#### 2.2.1 Transient Network Failures

**Definition**: Timeouts, connection refused, temporary DNS issues.

**Requirement**: DnsProvider MUST implement retry with exponential backoff.

**Parameters**:
- Maximum attempts: **3** (configurable via `DdnsConfig::max_retries`)
- Initial backoff: **5 seconds** (configurable via `DdnsConfig::retry_delay_secs`)
- Backoff strategy: Exponential (multiply by 2 each attempt)

**Behavior**:
- Provider MUST log each retry attempt at `INFO` level
- After final failure, Provider MUST return `Error::DnsProvider(...)`
- Engine emits `EngineEvent::UpdateFailed` with retry count

**Prohibited**:
- Provider MUST NOT retry indefinitely
- Provider MUST NOT implement custom backoff without documentation

#### 2.2.2 Authentication Failures

**Definition**: 401/403 responses, invalid API token, permission denied.

**Requirement**: Provider MUST fail immediately with no retry.

**Behavior**:
- Provider returns `Error::Authentication(...)`
- Engine emits `EngineEvent::UpdateFailed` with error details
- Engine does NOT retry this record

**Prohibited**:
- Provider MUST NOT retry auth failures
- Provider MUST NOT fall back to other auth mechanisms

#### 2.2.3 Rate Limiting

**Definition**: HTTP 429, provider-specific rate limit exceeded.

**Requirement**: Provider MUST retry with appropriate backoff.

**Behavior**:
- Provider extracts backoff duration from response (if provided)
- Provider waits specified duration before retry
- If no backoff specified, uses default exponential backoff
- Retry count applies (see 2.2.1)

**Prohibited**:
- Provider MUST NOT ignore rate limits
- Provider MUST NOT retry immediately

#### 2.2.4 Partial Updates (Multi-Record)

**Definition**: Provider supports batch updates, but partial batch fails.

**Requirement**: Provider MUST report granular failure.

**Behavior**:
- Provider returns `UpdateResult` for each successful record
- Provider returns `Error` for failed records
- Engine updates StateStore ONLY for successful records

**Prohibited**:
- Provider MUST NOT roll back successful updates
- Provider MUST NOT claim success if any record failed

---

### 2.3 StateStore Failures

#### 2.3.1 Unavailable at Startup

**Definition**: StateStore cannot be initialized (file missing, permission denied, connection failed).

**Requirement**: Daemon MUST refuse to start.

**Behavior**:
- `ddnsd` logs fatal error
- `ddnsd` exits with non-zero status code
- No attempt to run without StateStore

**Rationale**: Running without StateStore would cause redundant API calls after restart.

#### 2.3.2 Unavailable During Runtime

**Definition**: StateStore initialized successfully, but later writes fail.

**Behavior**:
- Engine logs error at `ERROR` level
- Engine continues operating with in-memory state
- Engine does NOT crash or shut down
- Engine does NOT retry writes indefinitely

**Subsequent behavior**:
- DNS updates continue normally
- State is not persisted
- On restart, StateStore may be stale

**Prohibited**:
- Engine MUST NOT block on StateStore writes
- Engine MUST NOT enter degraded mode for monitoring

#### 2.3.3 Read Failures

**Definition**: StateStore cannot read state (corruption, permission denied).

**Behavior**:
- If at startup: Treat as "Unavailable at Startup" (fail-fast)
- If during runtime: Treat as "Unavailable During Runtime" (continue in-memory)

**Prohibited**:
- Engine MUST NOT guess or fabricate state

#### 2.3.4 Corrupted State File

**Definition**: File exists but cannot be parsed (invalid JSON, schema mismatch).

**Requirement**: Daemon MUST refuse to start.

**Behavior**:
- `ddnsd` logs fatal error with corruption details
- `ddnsd` exits with non-zero status code
- Error message MUST indicate manual intervention required

**Rationale**: Automatic recovery risks amplifying errors.

---

### 2.4 Network Connectivity Failures

**Definition**: Intermittent network, provider API downtime, DNS resolution failure.

**Requirement**: Handled entirely by DnsProvider retry logic (see 2.2.1).

**Behavior**:
- Engine is unaware of network state
- Provider retries with backoff
- If all retries exhausted, Engine logs failure and continues monitoring

**Prohibited**:
- Engine MUST NOT implement separate network failure detection
- Engine MUST NOT pause IpSource monitoring on network failure

---

### 2.5 Daemon Crash / Restart

**Definition**: Process terminates unexpectedly (segfault, OOM, SIGKILL).

**Recovery mechanism**:
- StateStore persists last known IP and last successful update timestamp
- On restart, Engine loads state from StateStore
- Engine resumes monitoring from current IP
- Engine performs DNS update only if current IP differs from stored state

**Requirements**:
- StateStore writes MUST occur **before** confirming update success
- StateStore writes MUST be synchronous (durability required)
- In-memory updates are acceptable for performance, but MUST be flushed before returning success

**Prohibited**:
- StateStore MUST NOT buffer writes indefinitely
- Engine MUST NOT assume state was persisted if write failed

---

## 3. Cross-Component Failure Propagation

### 3.1 Error Propagation Rules

Errors propagate **upward** (from implementation to engine):

```
DnsProvider::update_record()
    → Returns Result<UpdateResult, Error>
    → Engine logs, retries if applicable, emits event
    → Engine continues monitoring (does NOT shut down)
```

Errors MUST NOT propagate **sideways**:

```
❌ WRONG: DnsProvider failure causes IpSource to stop
❌ WRONG: StateStore failure causes DnsProvider to stop
```

### 3.2 Isolation Guarantees

**Requirement**: Failure in one component MUST NOT prevent other components from operating.

**Examples**:
- DnsProvider failure for Record A MUST NOT block update for Record B
- StateStore write failure MUST NOT block IpSource monitoring
- IpSource stream error MUST NOT prevent StateStore flush on shutdown

**Exception**: IpSource stream termination causes graceful shutdown (by design).

---

## 4. Degraded Modes

### 4.1 Defined Degraded States

The system has **ONE** explicitly defined degraded mode:

**StateStore Write Failure Mode**:
- StateStore writes fail, but StateStore reads succeed
- System operates with in-memory state
- Updates continue normally
- State is not persisted

**Behavior**:
- Engine logs every write failure at `ERROR` level
- Engine does **not** emit special degraded-mode events
- On restart, stale state may cause redundant DNS update (acceptable)

**Exit condition**: StateStore becomes writable again.

### 4.2 Prohibited Degraded Modes

The following are **NOT** acceptable degraded states:

- **IpSource degraded mode**: IpSource MUST either work or terminate its stream
- **DnsProvider degraded mode**: Provider MUST either succeed or fail (no "partial success")
- **Partial StateStore mode**: StateStore MUST NOT return partial or stale state without error

---

## 5. Failure Detection and Observability

### 5.1 Logging Requirements

All failures MUST be logged:

| Failure Type | Log Level | Required Fields |
|--------------|-----------|-----------------|
| IpSource invalid event | WARN | event details, reason |
| IpSource stream terminated | ERROR | reason |
| DnsProvider transient failure | INFO | attempt number, error |
| DnsProvider final failure | ERROR | record name, error, retry count |
| DnsProvider auth failure | ERROR | record name, provider |
| StateStore unavailable at startup | ERROR | details |
| StateStore write failure | ERROR | record name, error |
| StateStore corrupted | ERROR | file path, parse error |

### 5.2 Event Emission

Engine MUST emit `EngineEvent` for:

- `UpdateFailed` - After all retries exhausted
- `Stopped` - On IpSource stream termination or shutdown signal

Engine MAY emit additional events for observability, but:
- MUST NOT emit events for successful operations (spam)
- MUST NOT emit events for transient failures (noise)

---

## 6. Failure Recovery Time Bounds

### 6.1 Expected Recovery Times

Under normal operation:

| Failure Type | Expected Recovery |
|--------------|-------------------|
| Transient DnsProvider failure | < 30 seconds (3 retries with backoff) |
| Network blip | < 30 seconds |
| StateStore transient write failure | Immediate (continues in-memory) |
| Daemon restart (crash) | < 5 seconds (startup + state load) |

### 6.2 Upper Bounds

No failure recovery MUST exceed **5 minutes** without:
- Logging a "prolonged failure" warning
- Emitting an event if applicable

Exception: Operator intervention required (e.g., auth failure).

---

## 7. Non-Goals in Failure Handling

The following are **explicitly NOT goals**:

- **High availability**: Daemon is single-process, no HA semantics
- **Automatic recovery from all failures**: Some failures require operator intervention
- **Byzantine fault tolerance**: No malicious actor model
- **Cross-provider fallback**: Provider failure is NOT mitigated by switching providers
- **StateStore replication**: Single backend, no replication

---

## 8. Implementation Checklist

Implementation MUST satisfy:

- [ ] IpSource stream terminates on unrecoverable error
- [ ] Engine discards invalid IpSource events with WARN log
- [ ] DnsProvider implements bounded retry with exponential backoff
- [ ] DnsProvider fails immediately on authentication errors
- [ ] DnsProvider implements rate limit backoff
- [ ] Daemon fails to start if StateStore unavailable
- [ ] Daemon fails to start if StateStore corrupted
- [ ] Engine continues in-memory on StateStore write failure
- [ ] Engine does NOT retry indefinitely
- [ ] All failures logged at appropriate level
- [ ] Engine events emitted for significant failures

---

## 9. Evolution Rules

Changes to this document:

- MUST NOT introduce new degraded modes without justification
- MUST NOT relax fail-fast requirements (StateStore at startup)
- MUST NOT violate PERFORMANCE.md constraints (e.g., no retry storms)
- MUST be reflected in implementation tests

---

This document is part of the authoritative architecture.
See: ARCHITECTURE.md, PERFORMANCE.md, .ai/AI_CONTRACT.md
