# AI_CONTRACT.md

This document defines **non-negotiable constraints** for all AI-assisted
development in this repository.

Any AI system (IDE agent, code generator, refactoring assistant, reviewer)
**MUST follow this contract** when generating or modifying code.

Violations of this contract are considered architectural bugs.

---

## 1. Project Identity (Immutable)

- Project name: **ddns**
- Repository: https://github.com/ddns-lab/ddns
- Language: **Rust**
- Type: **system-level, event-driven Dynamic DNS engine**
- Core priorities:
  - Extreme performance
  - Extreme resource sensitivity
  - Long-term architectural stability

These properties are **not subject to reinterpretation**.

---

## 2. Core Architectural Principles (Hard Constraints)

### 2.1 Core-first Design

- `ddns-core` is the **authoritative implementation** of all DDNS logic.
- `ddnsd` (daemon) is a **thin integration layer only**.

AI MUST NOT:
- Move business logic into `ddnsd`
- Add DNS logic, IP comparison, retry logic, or provider logic into `ddnsd`

AI MUST:
- Treat `ddns-core` as a reusable library intended for external integration.

---

### 2.2 Event-driven Is the Default

- IP change detection is **event-driven first**.
- Polling is allowed **only as a fallback mechanism**.

AI MUST NOT:
- Replace `IpSource::watch()` with polling-based loops
- Introduce periodic timers as the primary trigger mechanism

AI MAY:
- Add low-frequency polling as a safety net, explicitly documented as fallback

---

### 2.3 Strict Responsibility Boundaries

Each layer has a single responsibility:

- `IpSource`
  - Observes IP state
  - Emits events
  - Does NOT perform DNS updates

- `DdnsEngine`
  - Decides *whether* an update is needed
  - Coordinates state, providers, and backoff
  - Owns idempotency logic

- `DnsProvider`
  - Executes provider-specific API calls
  - Does NOT implement retry policy or scheduling

AI MUST NOT:
- Merge responsibilities across these boundaries
- Introduce cross-layer shortcuts for convenience

---

## 3. Public API Stability Rules (`ddns-core`)

### 3.1 Traits Are Contracts

The following traits are **architectural contracts**:

- `IpSource`
- `DnsProvider`
- `StateStore`

AI MUST NOT:
- Change existing trait method signatures casually
- Add methods without clear backward-compatibility reasoning
- Remove methods without a documented migration path

Any change to these traits requires:
- Explicit justification
- Corresponding documentation update

---

### 3.2 No Accidental API Growth

AI MUST NOT:
- Add public structs, enums, or functions "just in case"
- Expose internal types prematurely

Default visibility:
- `pub(crate)` unless there is a strong reason for `pub`

---

## 4. Provider Model Constraints

- Providers are **plugins**, not feature flags.
- Provider selection MUST be abstracted via a registry.

AI MUST NOT:
- Use `match provider_type { ... }` or similar hard-coded branching
- Embed Cloudflare- or vendor-specific logic into `ddns-core`

Each provider:
- Lives in its own crate
- Implements `DnsProvider`
- Has zero knowledge of other providers

---

## 5. Performance & Resource Constraints (Non-negotiable)

This project is **resource-sensitive by design**.

AI MUST:
- Avoid unnecessary allocations
- Avoid background threads without justification
- Prefer async I/O over blocking calls

AI MUST NOT:
- Introduce heavy dependencies without review
- Add caching layers without clear ownership and eviction rules
- Add logging that executes on hot paths by default

Correctness > features  
Performance regressions are considered **architectural failures**.

---

## 6. Configuration Rules

- All runtime configuration comes from **environment variables**
- Configuration is loaded once at startup

AI MUST NOT:
- Add interactive configuration
- Add hot-reload mechanisms
- Add embedded configuration UIs or files

---

## 7. What This Project Is NOT

AI MUST NOT attempt to turn this project into:

- A Web UI application
- A control plane
- A DNS server
- A general-purpose network monitoring agent
- A configuration management system

If a feature resembles any of the above, it is **out of scope**.

---

## 8. Documentation Synchronization Rule

If AI modifies:
- Architecture
- Public traits
- Responsibility boundaries

Then AI MUST:
- Update relevant documentation files
- Or explicitly state why documentation is unchanged

Silent architectural drift is forbidden.

---

## 9. When in Doubt

If there is uncertainty between:
- Simplicity vs extensibility
- Performance vs convenience
- Explicitness vs abstraction

AI MUST choose:
- Explicitness
- Performance
- Architectural clarity

---

## 10. Final Rule

This contract overrides:
- AI heuristics
- Convenience refactors
- "Common patterns" from other projects

The architecture of **ddns** is intentional.

Breaking this contract means breaking the system.

---

## 11. Trust Level Constraints

### 11.1 Extension Classification

Extensions are classified into three trust levels:

- **Trusted**: `ddns-core` (engine, registry, orchestration, state store implementations)
- **Semi-trusted**: IP sources (`ddns-ip-netlink`, future IP monitoring implementations)
- **Untrusted**: DNS providers (`ddns-provider-cloudflare`, future DNS provider implementations)

### 11.2 Capability Restrictions

Each trust level has strict capability restrictions. AI MUST enforce these limits.

**Trusted (ddns-core)**:
- ✅ Allocate (bounded)
- ✅ Spawn tasks (with clear lifecycle and shutdown guarantees)
- ✅ Perform I/O (coordinated access to providers, state store, IP sources)
- ✅ Own business logic (idempotency, retry coordination, decision-making)
- ❌ Must NOT violate trait boundaries
- ❌ Must NOT introduce polling loops as primary mechanism

**Semi-trusted (IP sources)**:
- ✅ Perform platform-specific I/O (Netlink, sockets, sysfs)
- ✅ Allocate (bounded)
- ⚠️ Spawn tasks (ONLY for event monitoring, NOT polling loops)
- ❌ Must NOT perform DNS updates
- ❌ Must NOT access state store directly
- ❌ Must NOT implement retry logic
- ❌ Must NOT spawn polling loops with `sleep()`
- ❌ Must NOT make decisions about when to update DNS

**Untrusted (DNS providers)**:
- ✅ Perform API calls only (HTTP/HTTPS to their endpoints)
- ⚠️ Allocate (minimize, prefer streaming)
- ✅ Parse provider-specific responses
- ❌ Must NOT spawn tasks or threads
- ❌ Must NOT implement retry logic or backoff
- ❌ Must NOT access state store
- ❌ Must NOT access other providers
- ❌ Must NOT make scheduling decisions
- ❌ Must NOT cache state beyond single request

### 11.3 Enforcement

AI MUST:
- Check trust level before adding capabilities to any component
- Document trust level compliance in code reviews
- Reference `docs/architecture/TRUST_LEVELS.md` for detailed definitions
- Verify that trait implementations include trust level documentation

AI MUST NOT:
- Grant untrusted components (providers) trusted capabilities (task spawning, retry logic, state access)
- Allow semi-trusted components (IP sources) to access state store or perform DNS updates
- Allow providers to implement their own retry logic
- Allow IP sources to use polling loops instead of event-driven mechanisms

### 11.4 Documentation Requirements

When creating or modifying trait implementations:

1. **Read the trust level documentation** in `docs/architecture/TRUST_LEVELS.md`
2. **Check the trait documentation** for trust level constraints (each core trait has a "Trust Level" section)
3. **Document compliance**:
   - If spawning tasks: document why and ensure shutdown safety
   - If performing I/O: document it's within trust level limits
   - If allocating: ensure it's bounded and necessary
4. **Run contract tests** to verify compliance

### 11.5 Violations Are Architectural Bugs

If a component violates its trust level:
- This is an **architectural bug**, not a style issue
- The violation **must be fixed** before merging
- The fix **must not** involve changing the trust level (that's an architectural decision)

See `docs/architecture/TRUST_LEVELS.md` for complete definitions, examples, and rationale.

---

## 12. Explicit Non-Goals

The ddns project explicitly forbids:

- Built-in metrics systems (Prometheus, statsd, etc.)
- Health check HTTP endpoints
- Embedded HTTP servers of any kind
- Background observability tasks

Rationale:
- ddns is designed to be externally supervised
- Liveness is defined by process existence and exit semantics
- Observability is log-driven and event-only

Any proposal introducing the above is a contract violation.
