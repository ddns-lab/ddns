# ARCHITECTURE.md

This document defines the **authoritative architecture** of the `ddns` project.

It describes:
- System boundaries
- Responsibility separation
- Data flow
- Non-negotiable architectural decisions

This document is intended for **humans**.
For AI-specific constraints, see `AI_CONTRACT.md`.

---

## 1. System Overview

`ddns` is a **system-level, event-driven Dynamic DNS engine** written in Rust.

Its purpose is to:
> Observe IP address changes and converge DNS records toward the desired state,
> with **minimal latency**, **minimal resource usage**, and **maximum correctness**.

The system is designed for:
- Long-running daemon usage
- Extremely low idle overhead
- Deterministic behavior
- Long-term architectural stability

---

## 2. High-level Architecture

The system is composed of two major parts:

```

+--------------------+
|      ddnsd         |  (daemon / integration layer)
+--------------------+
|
v
+--------------------+
|    ddns-core       |  (reusable library)
+--------------------+
|
v
+--------------------+
|  Providers / IP    |  (pluggable components)
+--------------------+

````

### Key principle

> **All domain logic lives in `ddns-core`.**  
> Everything else exists to support it.

---

## 3. Core vs Daemon Boundary

### 3.1 `ddns-core` (Library)

`ddns-core` is the **heart of the system**.

It is responsible for:
- Interpreting IP state changes
- Determining whether DNS updates are required
- Coordinating providers
- Ensuring idempotency and correctness
- Applying retry and backoff policies

`ddns-core`:
- Has **no knowledge** of how it is deployed
- Is reusable by external systems
- Contains no OS-specific startup logic

---

### 3.2 `ddnsd` (Daemon)

`ddnsd` is a **thin orchestration layer**.

Its responsibilities are strictly limited to:
- Reading configuration from environment variables
- Initializing the async runtime
- Constructing and wiring `ddns-core` components
- Starting the engine lifecycle

`ddnsd` MUST NOT:
- Contain business logic
- Perform DNS update decisions
- Implement provider-specific behavior

---

## 4. Event-driven Model

### 4.1 IP as a Stream of Events

IP state is treated as a **stream**, not as a periodically sampled value.

```text
IP change event
      ↓
Engine evaluation
      ↓
DNS convergence (if needed)
````

The primary mechanism is:

* `IpSource::watch()` → async stream of IP events

Polling:

* Is allowed only as a fallback
* Must never be the primary driver

---

### 4.2 Why Event-driven

This model ensures:

* Near-zero idle CPU usage
* Immediate reaction to IP changes
* Clear causal relationships
* Predictable behavior under load

---

## 5. Core Data Flow

The canonical data flow is:

```
IpSource
   ↓ (event)
DdnsEngine
   ↓ (decision)
StateStore
   ↓ (coordination)
ProviderRegistry
   ↓ (execution)
DnsProvider
```

Each step has a **single, well-defined responsibility**.

---

## 6. Core Components

### Trust Levels

Each component operates at a specific trust level with defined capabilities:

- **Core (Trusted)**: Full coordination capabilities (engine, registry, orchestration)
- **IP Sources (Semi-trusted)**: Platform-specific I/O only (Netlink, sockets)
- **Providers (Untrusted)**: API calls only (external DNS provider integrations)

See [TRUST_LEVELS.md](TRUST_LEVELS.md) for complete trust level definitions, including:
- What capabilities each trust level has (allocation, task spawning, I/O)
- What restrictions apply to each level
- Examples of correct and incorrect implementations
- How to classify new extensions

### 6.1 IpSource

Purpose:

* Observe IP state
* Emit changes as events

Responsibilities:

* Detect IPv4 / IPv6 changes
* Provide current IP snapshot
* Emit events via async streams

Non-responsibilities:

* DNS logic
* Provider interaction
* Retry or scheduling

---

### 6.2 DdnsEngine

Purpose:

* Act as the system coordinator

Responsibilities:

* Compare current IP vs previous state
* Determine whether updates are required
* Select applicable providers
* Enforce idempotency
* Apply retry and backoff policies

The engine is:

* Deterministic
* Stateless in memory, stateful via `StateStore`

---

### 6.3 StateStore

Purpose:

* Persist system state across restarts

Responsibilities:

* Store last observed IP
* Store last successful updates
* Track provider failures if needed

Design goals:

* Pluggable backend
* Explicit ownership of state
* No hidden caching

---

### 6.4 DnsProvider

Purpose:

* Execute DNS updates for a specific provider

Responsibilities:

* Interact with provider APIs
* Translate generic DNS records into provider-specific calls

Non-responsibilities:

* Scheduling
* Retry policy
* Cross-provider coordination

Each provider:

* Lives in its own crate
* Is isolated from other providers

---

### 6.5 ProviderRegistry

Purpose:

* Decouple engine from concrete providers

Responsibilities:

* Register providers
* Resolve providers by logical name
* Provide dynamic extensibility

The registry prevents:

* Hard-coded provider branching
* Tight coupling to vendor logic

---

## 7. Configuration Model

* Configuration is supplied exclusively via environment variables
* Configuration is loaded once at startup
* No hot-reload or dynamic reconfiguration

This ensures:

* Predictable runtime behavior
* Compatibility with containers and systemd
* Reduced runtime complexity

---

## 8. Performance & Resource Model

The architecture is explicitly designed for:

* Minimal memory footprint
* Minimal CPU usage at idle
* Async I/O on all external interactions

Key constraints:

* No background work without justification
* No hidden threads
* No implicit polling loops

Performance regressions are treated as architectural issues.

---

## 9. Non-goals (Architectural)

The following are explicitly out of scope:

* Web UI
* Control plane or management UI
* Embedded DNS server
* Configuration hot-reload
* General-purpose network monitoring

These constraints are intentional and permanent.

---

## 10. Architectural Evolution

Changes to:

* Core component boundaries
* Public traits
* Data flow

Require:

* Explicit discussion
* Documentation updates
* Clear migration strategy

Architecture evolves deliberately, not accidentally.

---

## 11. Compile-Time Misuse Prevention

`ddns` uses Rust's visibility system to prevent common misuse patterns at compile time.

### 11.1 Visibility Restrictions

The following items are `pub(crate)` to prevent external misuse:

| Item | Purpose | Why Restricted |
|------|---------|----------------|
| `DdnsEngine::run_with_shutdown` | Contract testing | Prevents external code from interfering with engine lifecycle |
| `StateRecord::new()` | State record creation | Prevents external creation of malformed state records |

**Note**: Some items remain public for legitimate use:
- `Error::*()` constructors - Used by provider crates for error reporting
- `IpChangeEvent::new()` - Used by `IpSource` implementations and contract tests

### 11.2 Misuse Prevention Examples

#### ✅ Cannot Interfere with Engine Lifecycle

External code cannot access `run_with_shutdown`, preventing interference with engine lifecycle:

```rust
// ❌ This does NOT compile from external code:
let (tx, rx) = tokio::sync::oneshot::channel();
engine.run_with_shutdown(Some(rx)).await;
// Error: method `run_with_shutdown` is pub(crate)

// ✅ Correct: External code uses the standard run() method
engine.run().await; // Runs until Ctrl+C
```

#### ✅ Cannot Create Malformed State Records

External code cannot create `StateRecord` instances directly:

```rust
// ❌ This does NOT compile from external code:
let record = StateRecord::new(IpAddr::from([1, 2, 3, 4]));
// Error: constructor is pub(crate)

// ✅ Correct: State records are created internally by DdnsEngine
```

### 11.3 Factory Pattern Safety

Traits that should only be implemented internally use the "sealed trait" pattern via factory traits:

- `DnsProviderFactory` - Creates `DnsProvider` instances
- `IpSourceFactory` - Creates `IpSource` instances
- `StateStoreFactory` - Creates `StateStore` instances

These factories are registered with `ProviderRegistry`, preventing:
- Direct instantiation of providers/sources
- Bypassing the registry system
- Hard-coded provider type branching

### 11.4 No Double-Start Prevention

The engine cannot be started twice because:
1. `run()` takes `&self` (not `&mut self`) for ergonomic use
2. `run()` never returns until shutdown
3. Attempting to call `run()` twice would require awaiting the first call (which blocks)

This design prevents accidental parallel execution while remaining ergonomic for correct usage.

### 11.5 Provider State Isolation

Providers receive `&self` (not `&mut self`) in trait methods, preventing:
- Providers from mutating their own state during updates
- Providers from storing internal state that could diverge
- Race conditions in provider implementations

```rust
// Provider trait methods take &self, preventing mutation
async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult>;
                                        ^^^^^ Read-only self
```

This ensures providers remain **stateless** and **idempotent**, which is critical for:
- Thread-safe concurrent access
- Predictable behavior under retries
- No hidden side effects between calls

---

## 12. Load and Event Storm Resistance

`ddns` implements multiple safeguards against load and event storms to ensure bounded resource usage and prevent DNS API abuse.

### 12.1 Bounded Event Channel

The engine uses a **bounded channel** for events, preventing unbounded memory growth:

```rust
pub struct EngineConfig {
    /// Capacity of the internal event channel (default: 1000)
    pub event_channel_capacity: usize,

    /// Minimum interval between updates for the same record (default: 60 seconds)
    pub min_update_interval_secs: u64,
    ...
}
```

**Behavior when channel is full**:
- New events are **dropped** (not queued)
- A warning is logged: `"Event channel full, dropping event"`
- This prevents unbounded memory growth under extreme load

**Why bounded channels matter**:
- **Before**: Unbounded channel could accumulate unlimited events → OOM
- **After**: Bounded channel drops events under stress → memory stays bounded

### 12.2 Rate Limiting (Minimum Update Interval)

The engine enforces a **minimum interval** between DNS updates for each record:

**Configuration**:
- Default: 60 seconds between updates
- Per-record enforcement (independent per record)
- Set to 0 to disable (not recommended for production)

**IP Flapping Protection**:
```
Time 0s:   IP changes to 1.2.3.4 → DNS update ✓
Time 5s:   IP changes to 1.2.3.5 → Skipped (too soon)
Time 10s:  IP changes to 1.2.3.6 → Skipped (too soon)
Time 65s:  IP changes to 1.2.3.7 → DNS update ✓ (interval elapsed)
```

**Implementation**:
- Tracks last update timestamp per record in `StateStore`
- Checks elapsed time before each update
- Skips update if interval hasn't elapsed

### 12.3 Provider Slow Response Handling

**Current behavior** (sequential processing):
- Engine processes IP changes sequentially in single async task
- Slow provider responses block subsequent IP event processing
- No parallel DNS updates

**Why this is safe**:
- Rate limiting prevents excessive API calls even under fast IP changes
- Bounded channel prevents memory growth
- Slow provider simply delays processing (doesn't cause unbounded growth)

### 12.4 Memory Usage Bounds

Under any load scenario, memory usage is bounded by:

| Component | Bound | Reason |
|-----------|-------|--------|
| Event channel | `event_channel_capacity` × size_of(EngineEvent) | Bounded channel |
| Provider registry | Number of registered plugins | Static after startup |
| Records | Number of configured records | Static from config |
| State store | Records × (IP + timestamp) | One entry per record |

**Worst-case scenario**:
- 1000 events in channel (default capacity)
- 100 records configured
- ~10 KB for events + ~10 KB for state = **~20 KB bounded memory**

### 12.5 Storm Scenarios

#### Scenario 1: IP Flapping (Rapid IP Changes)

**Attack**: IP changes 1000 times per second

**System response**:
1. First change triggers DNS update
2. Next 59 seconds of changes are **rate-limited** (skipped)
3. After 60 seconds, next change triggers DNS update
4. **Result**: Maximum 1 DNS update per minute per record

#### Scenario 2: DNS Provider Slow Response

**Attack**: DNS provider takes 10 seconds to respond

**System response**:
1. IP changes occur during provider response
2. Events accumulate in bounded channel (up to capacity)
3. When channel is full, events are **dropped** with warning
4. Provider response completes, processing resumes
5. **Result**: Bounded memory, no OOM, some events dropped (logged)

#### Scenario 3: Sustained High Load

**Attack**: Sustained 100 IP changes per second for hours

**System response**:
1. First change triggers DNS update
2. Subsequent changes rate-limited (60-second interval)
3. Channel may fill briefly but events are dropped
4. **Result**: Steady state of 1 DNS update per minute per record

### 12.6 Configuration Tuning

For different deployment scenarios:

**High-frequency IP changes** (e.g., unstable network):
```rust
min_update_interval_secs: 300  // 5 minutes (reduce API calls)
event_channel_capacity: 100     // Smaller buffer (drops faster)
```

**Stable network** (e.g., datacenter):
```rust
min_update_interval_secs: 30   // 30 seconds (faster updates)
event_channel_capacity: 1000   // Default buffer
```

**API rate limits** (e.g., Cloudflare free tier):
```rust
min_update_interval_secs: 120  // 2 minutes (respect API limits)
event_channel_capacity: 500    // Moderate buffer
```

### 12.7 Trade-offs

**Bounded channels**:
- ✅ Pro: Prevents OOM under load
- ❌ Con: Events dropped under stress (acceptable trade-off)

**Rate limiting**:
- ✅ Pro: Prevents API storms
- ❌ Con: Slower reaction to rapid IP changes (acceptable for most use cases)

**Sequential processing**:
- ✅ Pro: Simple, predictable behavior
- ❌ Con: Slow provider blocks processing (mitigated by rate limiting)

### 12.8 Monitoring

Under load, watch for these log messages:

**Normal**:
- `"Record {name} updated too recently (Xs ago), skipping update"`
- Indicates rate limiting is working

**Warning**:
- `"Event channel full, dropping event"`
- Indicates processing is slower than event generation
- Consider increasing `event_channel_capacity` or reducing IP change rate

---

## 13. Multi-Provider & Multi-Record Semantics

The ddns system has well-defined semantics for handling multiple records and provider interactions.

### 13.1 Single Provider Architecture

**Current Design**: The engine supports **exactly one DNS provider** at a time.

```rust
pub struct DdnsEngine {
    provider: Box<dyn DnsProvider>,  // Single provider
    ...
}
```

**Rationale**:
- **Simplicity**: Single provider eliminates complex routing logic
- **Determinism**: No ambiguity about which provider handles which record
- **Performance**: No overhead from provider selection or load balancing
- **Trust level compliance**: Providers are "untrusted" - keeping them isolated is safer

**Configuration**:
```yaml
provider:
  type: cloudflare
  api_token: "..."
  zone_id: "..."

# All records use this provider:
records:
  - name: "example.com"
  - name: "www.example.com"
  - name: "api.example.com"
```

### 13.2 Multi-Record Update Semantics

When an IP change event occurs, the engine updates **all enabled records** with the new IP.

**Update Flow**:
```
IP Change Event (new_ip: 1.2.3.4)
    ↓
For each record in config.records (in order):
    ↓
1. Check if record is enabled
2. Check if provider supports this record
3. Check idempotency (is IP different?)
4. Check rate limiting (minimum interval elapsed?)
5. Update DNS record (with retries)
6. Update state store
    ↓
Continue to next record (even if current fails)
```

**Key Properties**:

1. **Sequential Processing**
   - Records updated **one at a time** in configuration order
   - No parallel updates
   - Predictable behavior

2. **Fault Isolation**
   - If record A fails, record B is still updated
   - Errors are logged but don't stop the batch
   - Each record is independent

3. **Deterministic Order**
   - Updates happen in **configuration order**
   - `Vec<RecordConfig>` preserves insertion order
   - No random or concurrent reordering

### 13.3 Provider Record Support

Each provider implements `supports_record()` to indicate capability:

```rust
trait DnsProvider {
    fn supports_record(&self, record_name: &str) -> bool;
    ...
}
```

**Behavior**:
- Engine checks `provider.supports_record()` before each update
- If provider doesn't support the record, engine skips it with a warning
- This allows providers to implement domain filtering (e.g., only `*.example.com`)

**Example**:
```rust
// Provider only supports example.com and subdomains
impl DnsProvider for MyProvider {
    fn supports_record(&self, record_name: &str) -> bool {
        record_name.ends_with("example.com") || record_name.ends_with(".example.com")
    }
}
```

### 13.4 Deterministic Behavior Guarantees

The system guarantees **deterministic behavior** under all conditions:

| Scenario | Behavior | Deterministic? |
|----------|----------|----------------|
| **Single IP change** | Records updated in config order | ✅ Yes |
| **Multiple IP changes** | Processed sequentially, order preserved | ✅ Yes |
| **Provider slow response** | Blocks until complete, then next record | ✅ Yes |
| **Record update fails** | Logged, next record still updated | ✅ Yes |
| **Rate limit hit** | Update skipped, next record processed | ✅ Yes |

**No Implicit Fan-Out**:
- One IP change → sequential record updates (not parallel)
- No hidden concurrent operations
- No task spawning for "performance"

### 13.5 Why No Multi-Provider?

**Architectural Reasons**:

1. **Core-First Design** (AI_CONTRACT.md §2.1)
   - `ddns-core` should remain simple and reusable
   - Multi-provider adds routing complexity to the engine

2. **Strict Boundaries** (AI_CONTRACT.md §2.3)
   - `DdnsEngine` decides **whether** to update
   - `DnsProvider` decides **how** to update
   - Multi-provider blurs this line (engine needs provider-specific logic)

3. **Trust Levels** (TRUST_LEVELS.md)
   - Providers are "untrusted" - isolation is critical
   - Multiple providers increase attack surface and complexity

4. **Performance**
   - Sequential updates are already sufficient for typical DDNS workloads
   - Parallel updates would require bounded concurrency (complexity)

**When Would Multi-Provider Make Sense?**

If you need multiple providers, consider:
1. **Different domains** → Run multiple `ddnsd` instances
2. **Provider failover** → Use external load balancer or proxy
3. **Hybrid cloud** → Separate instances per provider

### 13.6 Fault Isolation Examples

**Scenario: One Record Fails**

```text
Records: [A, B, C, D]
IP change: 1.2.3.4

Update A → Success ✓
Update B → Failure ✗ (logged, but continues)
Update C → Success ✓
Update D → Success ✓

Result: 3/4 records updated, 1 failure logged
```

**Scenario: Provider Doesn't Support Record**

```text
Records: [example.com, api.google.com, www.example.com]
Provider: Cloudflare (only supports example.com)

example.com → Checked → Updated ✓
api.google.com → Checked → Skipped ⚠️ (provider doesn't support)
www.example.com → Checked → Updated ✓

Result: 2/3 records updated, 1 skipped with warning
```

### 13.7 Idempotency Per Record

Each record has **independent idempotency** tracking:

```rust
// StateStore tracks last IP per record
state_store.get_last_ip("example.com")   // Some(1.2.3.4)
state_store.get_last_ip("www.example.com") // Some(1.2.3.4)
state_store.get_last_ip("api.example.com") // None (first update)
```

**Benefits**:
- Records can be added/removed independently
- Each record has its own update history
- No cross-record dependencies

### 13.8 Rate Limiting Per Record

The `min_update_interval_secs` applies **independently to each record**:

```text
Time 0s:   IP changes to 1.2.3.4
           example.com → Updated ✓ (last update: 0s)
           www.example.com → Updated ✓ (last update: 0s)

Time 5s:  IP changes to 1.2.3.5
           example.com → Skipped (5s ago < 60s interval)
           www.example.com → Skipped (5s ago < 60s interval)

Time 65s: IP changes to 1.2.3.6
           example.com → Updated ✓ (65s ago > 60s interval)
           www.example.com → Updated ✓ (65s ago > 60s interval)
```

**No Global Rate Limiting**:
- Each record has its own timer
- Records don't block each other via rate limiting
- Independent update schedules

### 13.9 Future Multi-Provider Design (If Needed)

If multi-provider support is added in the future, it should follow these principles:

**Option 1: Provider Per Record** (Requires Architectural Change)
```rust
struct RecordConfig {
    name: String,
    provider: String,  // NEW: Provider selector
    ...
}

struct DdnsEngine {
    providers: HashMap<String, Box<dyn DnsProvider>>,  // NEW: Multiple providers
    ...
}
```

**Option 2: Provider Groups** (Recommended)
```yaml
providers:
  cloudflare:
    type: cloudflare
    api_token: "..."
  route53:
    type: route53
    ...

records:
  - name: "example.com"
    provider: "cloudflare"  # Explicit assignment
  - name: "internal.example.com"
    provider: "route53"
```

**Non-Negotiable Constraints**:
1. Sequential updates must remain deterministic
2. Fault isolation between records must be preserved
3. No implicit fan-out or parallelization
4. Provider selection must be explicit in configuration
5. Trust level boundaries must be maintained

---

## 14. Summary

`ddns` is designed as:

* A **core engine**, not an application
* An **event-driven system**, not a polling loop
* A **long-lived infrastructure component**

This architecture prioritizes:

* Correctness
* Performance
* Clarity
* Longevity

Any implementation detail must serve these goals.

---

## 15. Versioning & Compatibility

For detailed information about versioning policies, compatibility guarantees, and migration guides, see [VERSIONING.md](VERSIONING.md).

**Quick Reference**:
- **Policy**: Semantic Versioning (SemVer 2.0.0)
- **Current Version**: 0.1.0 (pre-release)
- **Stable APIs**: Traits, Configs, Errors, StateRecord
- **Breaking Changes**: Require MAJOR version bump
- **Provider Compatibility**: Version constraints required in Cargo.toml

**Key Principle**: If it breaks existing providers or state files, it's a MAJOR version change.

See [VERSIONING.md](VERSIONING.md) for:
- Breaking change criteria
- Migration guides
- Release process
- Deprecation policy
- FAQ

