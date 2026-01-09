# PERFORMANCE.md

## Performance Philosophy

`ddns` is designed as a **long-running, event-driven daemon** with an extreme focus on:

- **Minimal resource usage**
- **Predictable latency**
- **Zero unnecessary work**
- **Clear performance boundaries**

Performance is not an afterthought or a later optimization phase.
It is a **first-class design constraint** baked into architecture, APIs, and contracts.

This project prioritizes **long-term performance stability** over short-term throughput gains.

---

## Core Principles

### 1. Event-Driven, Not Polling

- The system MUST be driven by **external events**:
  - IP address changes
  - Explicit timers (only when unavoidable)
- Polling-based designs are explicitly discouraged.

Rationale:
- Polling wastes CPU cycles.
- Polling introduces latency ambiguity.
- Polling scales poorly on low-power devices.

---

### 2. No Work Without State Change

The engine MUST NOT perform DNS updates unless:

- The observed IP differs from the last known state
- The DNS provider state is confirmed to be out-of-sync

This includes:
- No redundant API calls
- No periodic “refresh” updates unless explicitly required by provider semantics

---

### 3. Memory Is a Budget, Not a Cache

- Memory usage should remain **flat over time**
- No unbounded collections
- No hidden buffering inside async streams

Preferred patterns:
- Small, bounded state
- Explicit backpressure
- Streaming instead of accumulation

---

### 4. Predictable Latency Over Raw Throughput

The expected workload of `ddns` is **low-frequency but latency-sensitive**.

Optimizations should favor:
- Fast reaction to IP change events
- Low jitter
- Avoiding tail latency spikes

High throughput (e.g. handling thousands of events per second) is **not a primary goal**.

---

### 5. Zero Background Chatter

When idle:
- CPU usage should approach **zero**
- No periodic logs
- No heartbeat tasks unless strictly required

The ideal idle state is:
> “Nothing happens until something changes.”

---

## Async Runtime Constraints

### Tokio Usage

- Tokio is used as the async runtime.
- Only essential Tokio features should be enabled.
- No `spawn` without clear ownership and lifecycle guarantees.

Guidelines:
- Prefer structured concurrency
- Avoid detached background tasks
- Every spawned task must have a clear shutdown path

---

### Streams and Backpressure

- `IpSource::watch()` returns a `Stream`
- Streams must:
  - Be cancel-safe
  - Respect downstream backpressure
  - Avoid buffering unbounded events

---

## Allocation Policy

### Heap Allocation

- Heap allocation is allowed, but must be:
  - Intentional
  - Bounded
  - Documented if non-obvious

Avoid:
- Allocation in hot paths
- Repeated allocation inside event loops

Prefer:
- Reuse
- Small structs
- `Arc` only when sharing is required

---

### Trait Objects

Trait objects are used intentionally for extensibility:

- `IpSource`
- `DnsProvider`
- `StateStore`

Guidelines:
- Dynamic dispatch cost is acceptable at system boundaries
- Avoid trait objects in inner loops
- Do not introduce generic explosion solely for micro-optimizations

---

## I/O and Network Behavior

### DNS Provider Calls

- Network calls are the **dominant cost**
- The engine MUST:
  - Minimize call count
  - Avoid retries without clear retry policy
  - Never retry blindly in tight loops

Backoff strategies:
- Explicit
- Bounded
- Provider-specific if needed

---

### State Persistence

- StateStore implementations should be:
  - Lightweight
  - Non-blocking
  - Durable enough to prevent redundant updates after restart

Durability is important, but not at the cost of:
- Blocking the main event loop
- Introducing heavy sync I/O

---

## Logging and Observability

### Logging

- Logging is for operators, not for debugging every step
- Default log level should be `INFO` or lower
- High-frequency paths MUST NOT log per event

Guidelines:
- Log state transitions, not steady state
- Errors must be logged
- Success paths should be mostly silent

---

### Metrics

Metrics are optional and SHOULD NOT:
- Introduce background tasks by default
- Allocate excessively
- Affect hot-path latency

If metrics are enabled:
- They must be explicitly opt-in
- They must be cheap to collect

---

## Performance Testing Philosophy

`ddns` does NOT aim for synthetic benchmarks like:
- Requests per second
- Maximum throughput

Instead, testing should focus on:
- Idle resource usage
- Reaction latency to IP changes
- Memory stability over long runtimes (days/weeks)

---

## Non-Goals

Explicit non-goals help protect performance long-term.

`ddns` does NOT aim to:
- Be a general-purpose DNS management system
- Support complex orchestration workflows
- Optimize for massive multi-tenant workloads
- Include a built-in web UI

These concerns belong in higher-level systems built **on top of** `ddns-core`.

---

## Evolution Rules

Any change that:
- Adds background tasks
- Introduces polling
- Increases idle CPU usage
- Adds unbounded buffering

MUST:
1. Be explicitly justified
2. Be documented
3. Be reviewed with performance impact as a first-class concern

---

## Final Note

> Performance is a feature.
> Predictability is a feature.
> Doing nothing efficiently is the hardest problem.

If a future design decision conflicts with this document,
**the document wins unless it is intentionally revised.**

