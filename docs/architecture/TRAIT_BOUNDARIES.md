# Trait Boundary Enforcement

This document defines the strict responsibility boundaries for all trait implementations in the ddns system. These boundaries are **architectural constraints** defined in `.ai/AI_CONTRACT.md` §2.3.

## Related Documents

- **[TRUST_LEVELS.md](TRUST_LEVELS.md)**: Defines what capabilities each layer is allowed to use (allocation, task spawning, I/O)
- **[AI_CONTRACT.md](../.ai/AI_CONTRACT.md)**: Authoritative architectural constraints
- **[ARCHITECTURE.md](ARCHITECTURE.md)**: High-level system architecture

**Key distinction**:
- **TRAIT_BOUNDARIES.md** (this file): Defines *responsibilities* (what to do)
- **TRUST_LEVELS.md**: Defines *capabilities* (what you can use)

Together, these documents ensure architectural integrity.

## Core Principle

Each trait has a **single responsibility**. Implementations MUST NOT exceed this responsibility.

## Trait Boundaries

### IpSource

**Responsibility**: Observe IP state and emit events

**Allowed**:
- ✅ Monitor network interfaces
- ✅ Detect IP address changes
- ✅ Emit `IpChangeEvent` via stream
- ✅ Return current IP via `current()`
- ✅ Filter by interface or IP version
- ✅ Select best IP from multiple addresses

**Forbidden**:
- ❌ Perform DNS updates (owned by `DdnsProvider`)
- ❌ Implement retry logic (owned by `DdnsEngine`)
- ❌ Spawn background tasks (violates event-driven architecture)
- ❌ Cache state beyond current IP (owned by `StateStore`)
- ❌ Make HTTP requests (use `IpSourceConfig::Http` instead)

**Trait Methods**:
```rust
async fn current(&self) -> Result<IpAddr, Error>;
fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>>;
fn version(&self) -> Option<IpVersion>;
```

**Detection of Violations**:
- Contract test `idle_no_background_polling` will detect polling
- Contract test `idle_no_periodic_wakeups` will detect background tasks
- Contract test `one_ip_change_triggers_exactly_one_dns_update` will detect DNS updates from IpSource

---

### DnsProvider

**Responsibility**: Execute provider-specific API calls

**Allowed**:
- ✅ Update DNS records via provider API
- ✅ Get current record metadata
- ✅ Check if record is supported
- ✅ Return provider name for logging
- ✅ Handle provider-specific errors

**Forbidden**:
- ❌ Implement retry policy or scheduling (owned by `DdnsEngine`)
- ❌ Spawn background tasks or threads (violates shutdown determinism)
- ❌ Implement state management (owned by `StateStore`)
- ❌ Poll for IP changes (owned by `IpSource`)
- ❌ Decide whether update is needed (owned by `DdnsEngine`)

**Trait Methods**:
```rust
async fn update_record(&self, record_name: &str, new_ip: IpAddr) -> Result<UpdateResult, Error>;
async fn get_record(&self, record_name: &str) -> Result<RecordMetadata, Error>;
fn supports_record(&self, record_name: &str) -> bool;
fn provider_name(&self) -> &'static str;
```

**Detection of Violations**:
- Contract test `retries_can_be_disabled_via_config` will detect provider-owned retries
- Contract test `no_future_leaks_after_shutdown` will detect background tasks
- Contract test `one_ip_change_triggers_exactly_one_dns_update` will detect multiple update attempts

---

### StateStore

**Responsibility**: Persistent state management for idempotency

**Allowed**:
- ✅ Store and retrieve last known IP
- ✅ Store and retrieve full state records
- ✅ Flush pending changes to disk
- ✅ List all records
- ✅ Delete records

**Forbidden**:
- ❌ Implement retry logic (owned by `DdnsEngine`)
- ❌ Spawn background tasks or threads
- ❌ Make DNS updates (owned by `DnsProvider`)
- ❌ Monitor IP changes (owned by `IpSource`)
- ❌ Decide when to update (owned by `DdnsEngine`)

**Trait Methods**:
```rust
async fn get_last_ip(&self, record_name: &str) -> Result<Option<IpAddr>, Error>;
async fn get_record(&self, record_name: &str) -> Result<Option<StateRecord>, Error>;
async fn set_last_ip(&self, record_name: &str, ip: IpAddr) -> Result<(), Error>;
async fn set_record(&self, record_name: &str, record: &StateRecord) -> Result<(), Error>;
async fn delete_record(&self, record_name: &str) -> Result<(), Error>;
async fn list_records(&self) -> Result<Vec<String>, Error>;
async fn flush(&self) -> Result<(), Error>;
```

**Detection of Violations**:
- Contract test `shutdown_flushes_state` verifies flush is called exactly once
- Contract test `no_future_leaks_after_shutdown` will detect background tasks

---

## DdnsEngine (Orchestrator)

**Responsibility**: Coordinate IP changes, state, and DNS updates

**Allowed**:
- ✅ Decide whether update is needed (idempotency check)
- ✅ Coordinate state, providers, and retries
- ✅ Emit events for observability
- ✅ Implement retry policy (with configurable max_retries)
- ✅ Handle shutdown gracefully

**Forbidden**:
- ❌ Spawn background tasks (violates shutdown determinism)
- ❌ Implement provider-specific logic (owned by `DnsProvider`)
- ❌ Implement IP monitoring logic (owned by `IpSource`)
- ❌ Bypass state store for idempotency

---

## Negative Tests (Boundary Violations)

The following contract tests act as **negative tests** - they fail when boundaries are violated:

| Boundary | Test | What It Detects |
|----------|------|-----------------|
| IpSource: No polling | `idle_no_background_polling` | IpSource polling periodically |
| IpSource: No background tasks | `idle_no_periodic_wakeups` | IpSource spawning timers/tasks |
| DnsProvider: No retries | `retries_can_be_disabled_via_config` | Provider-owned retry logic |
| DnsProvider: No background tasks | `no_future_leaks_after_shutdown` | Provider spawning tasks |
| Engine: Event-driven only | `one_ip_change_triggers_exactly_one_dns_update` | Extra update attempts |
| StateStore: Proper flush | `shutdown_flushes_state` | Missing or extra flush calls |

---

## Implementation Guidelines

### When Implementing a Trait

1. **Read the trait documentation** - Understand what's allowed
2. **Read AI_CONTRACT.md §2.3** - Understand the architectural constraints
3. **Run contract tests** - Verify your implementation doesn't violate boundaries
4. **Keep it minimal** - Only implement what the trait requires
5. **No hidden behavior** - Don't add background tasks, retries, or polling

### When Reviewing a Pull Request

1. Check for `tokio::spawn` - Should not exist in trait implementations
2. Check for `loop` with delays - Likely indicates polling
3. Check for retry logic - Should only exist in `DdnsEngine`
4. Check for state management - Should only exist in `StateStore`
5. Run contract tests - They will catch most violations

---

## Consequences of Violation

If a trait implementation violates its boundary:

1. **Contract tests will fail** - This is intentional
2. **Architecture is broken** - The system no longer guarantees its properties
3. **Bugs will occur** - E.g., provider retries causing API storms
4. **PR will be rejected** - Violations must be fixed before merging

---

## Examples of Violations

### ❌ WRONG: Provider with Retry Logic

```rust
// BAD: This violates the boundary
impl DnsProvider for MyProvider {
    async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
        let mut attempts = 0;
        loop {
            match self.do_update(record, ip).await {
                Ok(result) => return Ok(result),
                Err(e) if attempts < 3 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

**Why it's wrong**: Retry policy is owned by `DdnsEngine`, not providers.

**Fix**: Remove retry logic. Let the engine handle retries.

---

### ✅ CORRECT: Provider Without Retry

```rust
// GOOD: Single attempt, engine handles retries
impl DnsProvider for MyProvider {
    async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
        // Make ONE API call
        let response = self.client.put(&format!("/records/{}", record))
            .json(&serde_json::json!({ "content": ip }))
            .send()
            .await?;

        // Return result (success or failure)
        // Engine will retry if needed
        Ok(UpdateResult::Updated { ... })
    }
}
```

**Why it's correct**: Provider executes a single API call. Engine decides whether to retry.

---

### ❌ WRONG: IpSource with DNS Updates

```rust
// BAD: This violates the boundary
impl IpSource for MySource {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let stream = async_stream::stream! {
            loop {
                let ip = get_current_ip().await;
                let previous = self.state_store.get_last_ip("example.com").await;

                if ip != previous {
                    // BAD: IpSource shouldn't update DNS
                    self.provider.update_record("example.com", ip).await;
                    yield IpChangeEvent::new(ip, previous);
                }

                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        };
        Box::pin(stream)
    }
}
```

**Why it's wrong**:
1. IpSource is calling `provider.update_record()` (wrong layer)
2. IpSource is accessing `state_store` (wrong layer)
3. IpSource is polling with `sleep` (not event-driven)

**Fix**: IpSource should only emit events. Engine handles the rest.

---

### ✅ CORRECT: IpSource Without DNS Updates

```rust
// GOOD: Only emits events
impl IpSource for MySource {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Spawn a single monitoring task
        tokio::spawn(async move {
            loop {
                let ip = get_current_ip().await;
                tx.send(IpChangeEvent::new(ip, None));
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });

        Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
    }
}
```

**Wait, this is still wrong** - it's polling!

**Truly CORRECT**: Use Netlink events or another event-driven mechanism.

---

## Enforcement

Boundaries are enforced by:

1. **Contract tests** - Fail when boundaries are violated
2. **Code review** - Humans check for violations
3. **Documentation** - This file clarifies what's allowed
4. **AI_CONTRACT.md** - Immutable architectural constraints

If you're unsure whether something violates a boundary, ask:
- "Is this the responsibility of my trait?"
- "Will the contract tests pass?"
- "Does AI_CONTRACT.md allow this?"
