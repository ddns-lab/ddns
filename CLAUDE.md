# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## ⚠️ MANDATORY: Read AI_CONTRACT.md First

**Before making any changes to this codebase, you MUST read and fully understand**

👉 [`.ai/AI_CONTRACT.md`](.ai/AI_CONTRACT.md) 👈

The AI_CONTRACT.md defines **non-negotiable architectural constraints** for this project.

### Critical Constraints from AI_CONTRACT.md

1. **Core-first Design**: `ddns-core` is authoritative, `ddnsd` is a thin integration layer only
2. **Event-driven Default**: IP monitoring is event-driven first, polling only as fallback
3. **Strict Boundaries**: Never merge responsibilities across `IpSource`, `DdnsEngine`, `DnsProvider`
4. **Provider Plugin Model**: Use registry, never hard-coded `match provider_type { ... }`
5. **Performance First**: Resource-sensitive design, avoid unnecessary allocations/background threads
6. **Config via Env Vars Only**: No config files, no hot-reload, no interactive setup
7. **Scope Boundaries**: This is NOT a web UI, control plane, DNS server, or monitoring agent

**Any architectural change MUST update AI_CONTRACT.md or related documentation.**

---

## Project Overview

An event-driven Dynamic DNS system built with Rust, designed for high performance and minimal resource consumption. The system monitors IP address changes (via Linux Netlink) and automatically updates DNS records through provider APIs (e.g., Cloudflare).

## Architecture

### Monorepo Structure

This is a Cargo workspace with multiple crates:

```
crates/
├── ddns-core/              # Core library (traits, engine, registry)
├── ddnsd/                  # Daemon binary
├── ddns-provider-cloudflare/   # Cloudflare DNS provider
└── ddns-ip-netlink/        # Netlink IP source (Linux)
```

### Core Components

**`ddns-core`** - Core library with:
- **Traits**: `IpSource`, `DnsProvider`, `StateStore`
- **Engine**: `DdnsEngine` - Orchestrates IP change → DNS update flow
- **Registry**: `ProviderRegistry` - Plugin-based provider/IP source registration
- **Config**: `DdnsConfig`, `ProviderConfig`, `IpSourceConfig`

**Event Flow**:
1. `IpSource::watch()` yields `IpChangeEvent`
2. Engine checks `StateStore` for idempotency
3. If changed, calls `DnsProvider::update_record()`
4. On success, updates `StateStore`

### Key Design Principles

1. **Core-first**: `ddns-core` is the authoritative implementation, reusable as a library
2. **Event-driven**: IP monitoring via async streams, polling only as fallback (per AI_CONTRACT.md §2.2)
3. **Plugin architecture**: `ProviderRegistry` for dynamic provider registration (no hard-coded branching)
4. **Strict boundaries**:
   - `IpSource`: Observes IP state, emits events only
   - `DdnsEngine`: Decides whether update is needed, owns idempotency and backoff
   - `DnsProvider`: Executes provider-specific API calls only
5. **Resource-sensitive**: Minimal allocations, async I/O over blocking calls
6. **Idempotency**: `StateStore` prevents unnecessary API calls and enables crash recovery

## Build and Development

```bash
# Build all crates
cargo build

# Build with optimizations
cargo build --release

# Run daemon (after building)
cargo run --bin ddnsd

# Run tests
cargo test

# Run tests for specific crate
cargo test -p ddns-core

# Format code
cargo fmt

# Run linter
cargo clippy

# Check without building
cargo check
```

### Build Features

The `ddnsd` daemon has optional features:
- `cloudflare` - Enable Cloudflare provider
- `netlink` - Enable Netlink IP source
- `all` - Enable all providers/sources

```bash
# Build with all features
cargo build --bin ddnsd --features all
```

## Daemon Configuration

The `ddnsd` daemon is configured via environment variables:

### IP Source
- `DDNS_IP_SOURCE_TYPE` - Type: `netlink`, `http` (default: `netlink`)
- `DDNS_IP_SOURCE_INTERFACE` - Network interface (for netlink)
- `DDNS_IP_SOURCE_URL` - URL to fetch IP from (for http)
- `DDNS_IP_SOURCE_INTERVAL` - Poll interval in seconds (for http)

### DNS Provider
- `DDNS_PROVIDER_TYPE` - Provider type: `cloudflare` (default: `cloudflare`)
- `DDNS_PROVIDER_API_TOKEN` - API token (required)
- `DDNS_PROVIDER_ZONE_ID` - Zone ID (optional)

### Records
- `DDNS_RECORDS` - Comma-separated list: `example.com,www.example.com`

### State Store
- `DDNS_STATE_STORE_TYPE` - Type: `file`, `memory` (default: `file`)
- `DDNS_STATE_STORE_PATH` - Path to state file (for file store)

### Engine
- `DDNS_MAX_RETRIES` - Max retry attempts (default: 3)
- `DDNS_RETRY_DELAY_SECS` - Delay between retries (default: 5)
- `DDNS_LOG_LEVEL` - Log level: `trace`, `debug`, `info`, `warn`, `error` (default: `info`)

## Adding New Providers

To add a new DNS provider:

1. Create new crate: `crates/ddns-provider-{name}/`
2. Implement `DnsProvider` trait from `ddns-core`
3. Implement `DnsProviderFactory` for config-based creation
4. Export `register()` function to register with `ProviderRegistry`
5. Add as optional dependency to `ddnsd/Cargo.toml`
6. Add feature flag in `ddnsd/Cargo.toml`

## Adding New IP Sources

To add a new IP source:

1. Create new crate: `crates/ddns-ip-{name}/`
2. Implement `IpSource` trait from `ddns-core`
3. Implement `IpSourceFactory` for config-based creation
4. Export `register()` function
5. Add as optional dependency to `ddnsd/Cargo.toml`
6. Add feature flag in `ddnsd/Cargo.toml`

## Implementation Status

**Completed**:
- Core architecture and trait definitions
- `DdnsEngine` event-driven orchestration
- `ProviderRegistry` plugin system
- Daemon skeleton with env var config

**Skeleton Implementation** (API calls not implemented):
- `ddns-provider-cloudflare` - Structure defined, API calls TODO
- `ddns-ip-netlink` - Structure defined, Netlink operations TODO

**TODO**:
- Implement actual Cloudflare API calls (using `reqwest`)
- Implement Netlink socket operations (using `netlink-sys`)
- Add file-based state store implementation
- Add HTTP-based IP source implementation
- Proper error handling and retry logic

## Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests for specific crate
cargo test -p ddns-core
```

## Architectural Anti-Patterns (DO NOT DO)

Per AI_CONTRACT.md, the following are **explicitly forbidden**:

### ❌ Never Move Business Logic to `ddnsd`
```rust
// WRONG - This violates AI_CONTRACT.md §2.1
// ddnsd/src/main.rs
async fn update_dns_record(record: &str, ip: IpAddr) {
    // DNS logic belongs in ddns-core, not ddnsd!
}
```

### ❌ Never Hard-code Provider Branching
```rust
// WRONG - This violates AI_CONTRACT.md §4
// ddns-core/src/some_module.rs
match provider_type {
    "cloudflare" => /* ... */,
    "route53" => /* ... */,
    // Use ProviderRegistry instead!
}
```

### ❌ Never Add Polling as Primary Mechanism
```rust
// WRONG - This violates AI_CONTRACT.md §2.2
// Primary IP monitoring MUST be event-driven
async fn poll_ip_changes() {
    loop {
        let ip = get_current_ip().await;
        sleep(Duration::from_secs(60)).await; // Not default!
    }
}
```

### ❌ Never Merge Responsibilities
```rust
// WRONG - This violates AI_CONTRACT.md §2.3
// IpSource should NOT perform DNS updates
impl IpSource for MySource {
    fn watch(&self) -> impl Stream {
        // ... emit IP changes
        // WRONG: Don't call provider.update_record() here!
    }
}
```

### ❌ Never Add Config Files or Web UI
Per AI_CONTRACT.md §6 and §7:
- No config files (TOML, YAML, JSON)
- No hot-reload
- No embedded Web UI
- Environment variables only

### ❌ Never Add Heavy Dependencies Without Justification
Per AI_CONTRACT.md §5:
- Performance regressions are architectural failures
- Avoid unnecessary allocations
- Prefer async I/O over blocking calls

## License

Apache License 2.0
