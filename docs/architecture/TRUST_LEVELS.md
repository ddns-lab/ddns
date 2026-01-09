# Trust Levels

This document defines the **trust level classification system** for all ddns components.

Trust levels specify **what capabilities each layer is allowed to use**:
- Can it allocate memory?
- Can it spawn tasks or threads?
- Can it perform I/O?
- What state can it access?

This is distinct from **trait boundaries** (see [TRAIT_BOUNDARIES.md](TRAIT_BOUNDARIES.md)), which define **what responsibilities each layer owns**.

Together, these documents ensure:
- **Clear capability boundaries** - No confusion about what each layer can do
- **Architectural integrity** - Components cannot exceed their authorized capabilities
- **Future-proof design** - New extensions can be classified into existing trust levels

---

## Overview

The ddns system has three trust levels:

| Trust Level | Components | Purpose |
|-------------|------------|---------|
| **Trusted** | ddns-core (engine, registry, orchestration) | Full coordination capabilities |
| **Semi-trusted** | IP sources (ddns-ip-netlink, future implementations) | Platform-specific I/O only |
| **Untrusted** | DNS providers (ddns-provider-cloudflare, future implementations) | External API calls only |

---

## Capability Matrix

| Capability | Core (Trusted) | IP Sources (Semi-trusted) | Providers (Untrusted) |
|------------|----------------|---------------------------|----------------------|
| **Allocate** | ✅ Bounded | ✅ Bounded | ⚠️ Minimize |
| **Spawn Tasks** | ✅ With lifecycle | ⚠️ Event-monitoring only | ❌ Never |
| **Perform I/O** | ✅ Coordinated | ✅ Platform-specific | ✅ API calls only |
| **Access State Store** | ✅ | ❌ | ❌ |
| **Make Decisions** | ✅ | ❌ | ❌ |
| **Retry Logic** | ✅ | ❌ | ❌ |
| **DNS Updates** | ❌ (via provider) | ❌ | ✅ |
| **IP Monitoring** | ❌ (via source) | ✅ | ❌ |

---

## Level 1: Trusted (ddns-core)

### Components

- `DdnsEngine` - Orchestrates IP changes, state, and DNS updates
- `ProviderRegistry` - Manages plugin registration and creation
- `StateStore` implementations - Persistent state management
- Trait definitions (`IpSource`, `DnsProvider`, `StateStore`)

### Allowed Capabilities

✅ **Allocate**
- Bounded allocations for engine state, event streams, configuration
- No unbounded collections or unchecked growth

✅ **Spawn Tasks**
- Only with clear lifecycle and shutdown guarantees
- Structured concurrency patterns preferred
- Every spawned task must have a deterministic shutdown path

✅ **Perform I/O**
- Coordinated access to providers, state store, and IP sources
- Async I/O only (no blocking calls in hot paths)

✅ **Own Business Logic**
- Idempotency checks
- Retry coordination and backoff policies
- Decision-making (when to update, what to retry)
- Event emission for observability

✅ **Coordinate**
- Manage multi-provider scenarios
- Handle error recovery
- Enforce architectural boundaries

### Restrictions

❌ **Must NOT**:
- Violate trait boundaries (no merging responsibilities)
- Introduce polling loops as primary mechanism (event-driven only)
- Create unnecessary background chatter (idle CPU should approach zero)
- Make blocking I/O calls in hot paths

### Rationale

The core is the **authoritative implementation** of all DDNS logic. It needs full capabilities to:
- Coordinate between multiple extensions
- Enforce idempotency and correctness
- Manage system lifecycle and shutdown
- Provide observability and error recovery

Without these capabilities, the system could not guarantee correctness or performance.

---

## Level 2: Semi-Trusted (IP Sources)

### Components

- `ddns-ip-netlink` - Linux Netlink-based IP monitoring
- Future: UDP socket-based, BSD-specific, Windows-specific implementations

### Allowed Capabilities

✅ **Perform I/O**
- Read network state from kernel (Netlink, sockets, sysfs, /proc)
- Access platform-specific APIs
- Monitor network interface changes

✅ **Allocate**
- Bounded allocations for streams, events, address parsing
- No unbounded buffering or event queues

⚠️ **Spawn Tasks** (Conditional)
- **Allowed only if**:
  - Task has clear shutdown path (cancellation-safe)
  - Task is for **event monitoring only** (not polling)
  - Uses event-driven mechanisms (Netlink subscriptions, socket notifications)
- **Forbidden**: Spawned tasks that poll periodically with `sleep()` loops

✅ **Access Platform APIs**
- Linux Netlink sockets
- BSD socket APIs
- Platform-specific network interfaces

### Restrictions

❌ **Must NOT**:
- Perform DNS updates (owned by `DnsProvider`)
- Access state store directly (owned by `DdnsEngine`)
- Implement retry logic (owned by `DdnsEngine`)
- Spawn polling loops (use event-driven mechanisms)
- Make decisions about when to update DNS
- Access other IP sources or providers

### Rationale

IP sources need **platform-specific I/O access** to detect network changes, but must not cross into business logic or coordination responsibilities. They are **observers**, not **decision-makers**.

The "semi-trusted" designation reflects that:
- They need privileged access (kernel APIs, network state)
- They have limited scope (observation only)
- They must be isolated from business logic

### Examples

#### ✅ CORRECT: Event-driven IP source

```rust
impl IpSource for NetlinkSource {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Spawn task for Netlink event monitoring
        tokio::spawn(async move {
            // Subscribe to Netlink events (event-driven, not polling)
            let socket = netlink_socket();
            socket.subscribe_to_route_changes();

            loop {
                let event = socket.read_event().await; // Block on events, not sleep
                tx.emit(event);
            }
        });

        Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
    }
}
```

**Why it's correct**:
- Task waits for Netlink events (event-driven)
- No polling loop with `sleep()`
- Task has clear shutdown path (drop tx, socket closes)

#### ❌ WRONG: Polling IP source

```rust
// BAD: This violates trust level constraints
impl IpSource for PollingSource {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = IpChangeEvent> + Send + 'static>> {
        let stream = async_stream::stream! {
            loop {
                let ip = self.get_current_ip().await; // Poll
                yield IpChangeEvent::new(ip, None);
                tokio::time::sleep(Duration::from_secs(60)).await; // WRONG!
            }
        };
        Box::pin(stream)
    }
}
```

**Why it's wrong**:
- Uses polling loop instead of event-driven mechanism
- Wastes CPU cycles even when no changes occur
- Adds latency (up to 60 seconds) before detecting changes
- Violates AI_CONTRACT.md §2.2 (event-driven is default)

---

## Level 3: Untrusted (DNS Providers)

### Components

- `ddns-provider-cloudflare` - Cloudflare API integration
- Future: Route53, DigitalOcean, GoDaddy, etc.

### Allowed Capabilities

✅ **Perform I/O**
- HTTP/HTTPS API calls to their specific endpoints only
- TLS connections for secure communication
- Response parsing and error handling

⚠️ **Allocate**
- Minimize allocations (prefer streaming for large responses)
- Reuse buffers where possible
- Avoid allocations in hot paths

✅ **Parse Responses**
- Provider-specific response parsing
- Error code translation
- Metadata extraction (TTL, record IDs, etc.)

### Restrictions

❌ **Must NOT**:
- Spawn tasks or threads (violates shutdown determinism)
- Implement retry logic (owned by `DdnsEngine`)
- Access state store (owned by `DdnsEngine`)
- Access other providers (must be isolated)
- Make scheduling decisions (owned by `DdnsEngine`)
- Cache state beyond single request (owned by `StateStore`)
- Decide whether an update is needed (owned by `DdnsEngine`)
- Perform any I/O other than API calls to their endpoints

### Rationale

DNS providers are **external integrations** that should be:
- **Isolated**: No knowledge of other providers or system state
- **Stateless**: No persistent state between requests
- **Single-shot**: Execute one API call per invocation
- **Deterministic**: Same input → same output, no hidden behavior

The "untrusted" designation reflects that:
- They communicate with external systems (potential security boundary)
- They have the most restricted capabilities
- They must not affect system behavior beyond their API calls

### Examples

#### ✅ CORRECT: Stateless provider

```rust
impl DnsProvider for CloudflareProvider {
    async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
        // Single API call
        let response = self.http_client
            .put(format!("/zones/{}/dns_records/{}", self.zone_id, record))
            .json(&serde_json::json!({ "content": ip.to_string() }))
            .send()
            .await?;

        // Parse response
        if response.status().is_success() {
            let result = response.json::<CloudflareResponse>().await?;
            return Ok(UpdateResult::Updated { ... });
        }

        // Return error (engine will retry if needed)
        Err(Error::provider_error("API call failed"))
    }
}
```

**Why it's correct**:
- Single API call (no retry logic)
- No task spawning
- No state caching
- Returns success or failure (engine decides what to do)

#### ❌ WRONG: Provider with retry logic

```rust
// BAD: This violates trust level constraints
impl DnsProvider for BadProvider {
    async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
        let mut attempts = 0;
        loop {
            match self.do_update(record, ip).await {
                Ok(result) => return Ok(result),
                Err(e) if attempts < 3 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_secs(1)).await; // WRONG!
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

**Why it's wrong**:
- Implements retry logic (owned by `DdnsEngine`)
- Spawns implicit task with `sleep()` (violates shutdown determinism)
- Engine cannot control retry policy
- Can cause API storms if multiple providers retry independently

---

## Enforcement and Validation

### Code Review Checklist

When reviewing code for each trust level:

#### For Core (Trusted):
- [ ] Are all allocations bounded?
- [ ] Do spawned tasks have clear shutdown paths?
- [ ] Is I/O async-only?
- [ ] Are trait boundaries respected?

#### For IP Sources (Semi-trusted):
- [ ] Is it event-driven (not polling)?
- [ ] Are spawned tasks for monitoring only (not polling loops)?
- [ ] Does it access only platform-specific APIs?
- [ ] Does it avoid DNS updates, state access, and retry logic?

#### For DNS Providers (Untrusted):
- [ ] Are there no spawned tasks?
- [ ] Is there no retry logic?
- [ ] Does it access only its own API endpoints?
- [ ] Does it avoid state caching and coordination?

### Contract Tests

The following contract tests enforce trust level constraints:

| Test | Trust Level | What It Detects |
|------|-------------|-----------------|
| `idle_no_background_polling` | IP Sources | Polling loops in IP sources |
| `idle_no_periodic_wakeups` | IP Sources | Background tasks causing wakeups |
| `retries_can_be_disabled_via_config` | Providers | Provider-owned retry logic |
| `no_future_leaks_after_shutdown` | All | Undocumented spawned tasks |

### Documentation Requirements

All implementations must document:
1. Their trust level classification
2. What capabilities they use
3. Why they need those capabilities
4. How they ensure compliance with restrictions

---

## Classification Guide for New Extensions

### Is this a Trusted Component?

**Yes** if:
- It coordinates multiple extensions
- It makes decisions about system behavior
- It needs access to multiple subsystems

**Examples**: New engine types, orchestration logic, coordination primitives

**Where it lives**: `ddns-core` crate

---

### Is this a Semi-Trusted Component?

**Yes** if:
- It monitors system state (IP, network, hardware)
- It needs platform-specific API access
- It emits events but makes no decisions

**Examples**: New IP sources (FreeBSD socket, Windows API), hardware monitors

**Where it lives**: Separate crate (`ddns-ip-{name}`)

---

### Is this an Untrusted Component?

**Yes** if:
- It integrates with an external API
- It performs network I/O to external services
- It should be isolated from system state

**Examples**: New DNS providers, external service integrations

**Where it lives**: Separate crate (`ddns-provider-{name}`)

---

## Relationship to Other Documents

### TRAIT_BOUNDARIES.md

**TRAIT_BOUNDARIES.md** defines **responsibilities** (what to do):
- `IpSource`: Observe IP state, emit events
- `DnsProvider`: Execute provider-specific API calls
- `DdnsEngine`: Decide whether update is needed, coordinate state

**TRUST_LEVELS.md** defines **capabilities** (what you can use):
- **Trusted**: Can allocate, spawn tasks, perform coordinated I/O
- **Semi-trusted**: Can perform platform I/O, limited task spawning
- **Untrusted**: Can perform API calls only, no task spawning

Together they ensure:
- Components don't take on responsibilities outside their trait
- Components don't use capabilities beyond their trust level

### AI_CONTRACT.md

**AI_CONTRACT.md** makes trust levels **non-negotiable architectural constraints** (§11):
- AI MUST check trust level before adding capabilities
- AI MUST NOT grant untrusted components trusted capabilities
- Violations are considered architectural bugs

This document provides the detailed definitions referenced by AI_CONTRACT.md.

---

## FAQ

### Q: Why are DNS providers "untrusted"?

**A**: Not because they're malicious, but because they have the **most restricted capabilities**. They communicate with external systems (potential security boundary) and must be isolated from system state to prevent:
- Unauthorized state access
- Implicit coordination between providers
- Hidden side effects

### Q: Can an IP source open a file?

**A**: Yes, if it's needed for platform-specific I/O (e.g., reading `/proc/net/route` on Linux). However, it must:
- Use async I/O only
- Not cache file contents (owned by `StateStore`)
- Not make decisions based on file contents (owned by `DdnsEngine`)

### Q: Can the core spawn a task for a provider?

**A**: Yes, the core can spawn tasks to call providers. The provider itself must not spawn tasks. This separation ensures:
- Core controls task lifecycle
- Core can implement shutdown determinism
- Core can coordinate between multiple providers

### Q: What if a provider needs to retry?

**A**: The provider should return an error. The `DdnsEngine` will retry according to its configured policy. This ensures:
- Consistent retry behavior across providers
- Engine can control retry rate (preventing API storms)
- Engine can implement backoff policies

### Q: Can trust levels change?

**A**: Trust levels are **architectural boundaries**, not version numbers. They should not change casually. If a component genuinely needs different capabilities, it may indicate:
- The component is in the wrong category (e.g., should be part of core)
- The architecture needs revision (discuss and document)

---

## Summary

Trust levels ensure **architectural integrity** by:

1. **Explicit capabilities**: No ambiguity about what each layer can do
2. **Clear boundaries**: Components cannot exceed their authorized capabilities
3. **Future-proof design**: New extensions fit into existing trust levels
4. **Enforceable constraints**: AI_CONTRACT.md makes trust levels non-negotiable

When implementing a new component:
1. Determine its trust level using the classification guide
2. Read the allowed capabilities and restrictions
3. Document how your implementation complies
4. Run contract tests to verify compliance

**Violations of trust levels are architectural bugs.**
