# DDNS

[![CI](https://img.shields.io/github/actions/workflow/status/ddns-lab/ddns/ci.yml?branch=main&label=CI)](https://github.com/ddns-lab/ddns/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://www.rust-lang.org)
[![GitHub Release](https://img.shields.io/github/v/release/ddns-lab/ddns)](https://github.com/ddns-lab/ddns/releases/latest)
[![Stars](https://img.shields.io/github/stars/ddns-lab/ddns?style=social)](https://github.com/ddns-lab/ddns/stargazers)
[![Downloads](https://img.shields.io/github/downloads/ddns-lab/ddns/total.svg)](https://github.com/ddns-lab/ddns/releases)
[![Issues](https://img.shields.io/github/issues/ddns-lab/ddns)](https://github.com/ddns-lab/ddns/issues)
[![codecov](https://codecov.io/gh/ddns-lab/ddns/branch/main/graph/badge.svg)](https://codecov.io/gh/ddns-lab/ddns)

An event-driven Dynamic DNS system built with Rust, designed for high performance and **minimal resource consumption**.

## 🚀 Resource Efficiency

**Extreme resource efficiency through Rust's zero-cost abstractions and event-driven design:**

| Metric | Value | Comparison |
|--------|-------|-------------|
| **Binary Size** | **3.5 MB** | Smaller than a single HD photo |
| **Memory Usage** | **~13 MB RSS** | Fraction of Go/Python implementations (100-500MB+) |
| **Startup Time** | **~20 ms** | Near-instant, no JVM warmup |
| **CPU Idle** | **~0%** | Event-driven, no polling threads |
| **Static Linking** | Optional | Single binary deployment, no runtime deps |

**Why so efficient?**

- ✅ **Zero-cost abstractions**: Rust's compile-time optimization
- ✅ **Event-driven architecture**: No polling, no background threads
- ✅ **No garbage collection**: Deterministic memory usage, no GC pauses
- ✅ **Minimal runtime**: No VM, no interpreter, bare metal performance
- ✅ **Smart dependencies**: Only what you need, async I/O over blocking calls

## Project Goals

- **Extreme resource efficiency**: Minimal overhead, 3.5MB binary, ~13MB RAM
- **Event-driven**: React to IP changes instantly via Linux Netlink (no polling)
- **Zero-cost abstractions**: Rust's safety without performance penalty
- **Long-term stability**: Clear architecture, well-defined boundaries
- **Library-first**: Core logic reusable as a Rust library
- **Production-ready**: Comprehensive validation and error handling

## Features

- ✅ **Ultra-lightweight**: 3.5MB binary, ~13MB RAM, 20ms startup
- ✅ **Event-driven architecture**: React to network changes instantly via Linux Netlink (no polling)
- ✅ **Idempotency**: Prevents unnecessary DNS updates via state tracking
- ✅ **Provider plugin system**: Easy to add new DNS providers
- ✅ **Cloudflare integration**: Production-ready with auto-create, A/AAAA records
- ✅ **Dry-run mode**: Safe testing without making actual changes
- ✅ **Comprehensive error handling**: Clear error messages for all failure scenarios
- ✅ **Security-first**: API tokens never logged, env var config only
- ✅ **CI/CD**: GitHub Actions for testing, security auditing, and multi-platform builds

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  IpSource   │────▶│  DdnsEngine  │────▶│ DnsProvider │
│  (netlink)  │     │  (ddns-core) │     │ (cloudflare)│
└─────────────┘     └──────────────┘     └─────────────┘
                            │
                            ▼
                     ┌──────────────┐
                     │  StateStore  │
                     │ (idempotency)│
                     └──────────────┘
```

## Project Structure

```
ddns/
├── .ai/                         # AI development contracts
│   ├── AI_CONTRACT.md           # ⚠️ Non-negotiable architectural constraints
│   └── QUICK_START.md           # Quick reference for AI agents
├── .github/workflows/           # CI/CD pipelines
│   ├── ci.yml                   # Test, lint, security audit
│   └── release.yml              # Automated releases
├── crates/
│   ├── ddns-core/               # Core library (traits, engine, registry)
│   ├── ddnsd/                   # Daemon binary
│   ├── ddns-provider-cloudflare/ # Cloudflare DNS provider ✅
│   ├── ddns-ip-netlink/         # Netlink IP source (Linux) ✅
│   └── ddns-ip-http/            # HTTP IP source (fallback) ✅
├── docs/                        # Architecture documentation
│   └── PHASE_22_VALIDATION.md   # Cloudflare provider validation report
├── examples/                    # Example programs and validation tools
│   └── cloudflare-validation.rs # Real environment validation tool
├── install.sh                   # One-line installer (Linux)
├── CLAUDE.md                    # Comprehensive development guide
└── README.md
```

## Documentation

- **[`.ai/AI_CONTRACT.md`](.ai/AI_CONTRACT.md)** - Mandatory architectural constraints for all development
- **[`CLAUDE.md`](CLAUDE.md)** - Comprehensive development guide
- **[`docs/PHASE_22_VALIDATION.md`](docs/PHASE_22_VALIDATION.md)** - Cloudflare provider validation report
- **[`.ai/QUICK_START.md`](.ai/QUICK_START.md)** - Quick reference for contributors

## Implementation Status

### ✅ Production-Ready (v0.1.1)

**Core Components:**
- ✅ **Event-driven engine**: Async orchestration with idempotency & retry logic
- ✅ **Provider registry**: Plugin system for dynamic provider/IP source registration
- ✅ **State management**: File & Memory stores with atomic writes and backup recovery

**IP Sources:**
- ✅ **Netlink IP source** (`ddns-ip-netlink`): Linux kernel event-driven (RTM_NEWADDR/RTM_DELADDR)
- ✅ **HTTP IP source** (`ddns-ip-http`): Polling-based with configurable interval
- ✅ **IPv4/IPv6 support**: Auto-detection or explicit version selection

**DNS Providers:**
- ✅ **Cloudflare provider** (`ddns-provider-cloudflare`):
  - Automatic zone discovery
  - A/AAAA record updates (IPv4/IPv6)
  - **Auto-create records** (v0.1.1): Creates missing records automatically
  - Dry-run mode for safe testing
  - Comprehensive error handling and validation

**Daemon & Deployment:**
- ✅ **ddnsd binary**: Complete daemon with signal handling (SIGTERM/SIGINT)
- ✅ **Environment variable config**: No config files needed
- ✅ **Systemd integration**: `install.sh` with auto-start on boot
- ✅ **Automated releases**: GitHub Actions with multi-platform builds
- ✅ **Installer script**: One-line installation with upgrade support

**Testing & Quality:**
- ✅ **Architectural contract tests**: Event-driven, idempotency, retry logic
- ✅ **Comprehensive validation**: Real environment Cloudflare testing
- ✅ **CI/CD**: GitHub Actions for test, lint, security audit, releases

### 📋 Upcoming Features

- **Additional DNS providers**: Route53, DigitalOcean, Namecheap, etc.
- **macOS/Windows support**: Native IP change detection (FSEvents/WSA)
- **Web UI**: Optional dashboard for monitoring and configuration
- **Metrics export**: Prometheus integration for observability
- **Configuration profiles**: Multiple DNS provider support

## Quick Start

### One-Line Installation (Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

This will:
- Download the latest binary for your platform
- Install to `/usr/local/bin/ddnsd`
- Create systemd service with auto-start on boot
- Configure via `/etc/ddnsd/ddnsd.env`

See [`install.sh`](install.sh) for advanced options (non-interactive mode, custom paths, etc.).

### Manual Configuration

1. **Edit configuration:**
```bash
sudo vi /etc/ddnsd/ddnsd.env
```

2. **Configure your Cloudflare API token and records:**
```bash
# Cloudflare Configuration
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=your_api_token_here
DDNS_PROVIDER_ZONE_ID=your_zone_id_here

# Records to update (comma-separated)
# Format: name:type (type: A, AAAA, or Auto)
DDNS_RECORDS=example.com:A,www.example.com:AAAA

# IP Source Configuration
DDNS_IP_SOURCE_TYPE=netlink  # or "http"
# DDNS_IP_SOURCE_INTERFACE=eth0  # for netlink
# DDNS_IP_SOURCE_URL=https://icanhazip.com  # for http
# DDNS_IP_SOURCE_INTERVAL=300  # for http (seconds)
# DDNS_IP_SOURCE_VERSION=both  # Options: v4, v6, both
```

3. **Start the service:**
```bash
sudo systemctl start ddnsd
sudo systemctl enable ddnsd  # Auto-start on boot
```

4. **Check status and logs:**
```bash
sudo systemctl status ddnsd
sudo journalctl -u ddnsd -f  # Follow logs
```

### Build from Source

```bash
# Clone repository
git clone https://github.com/ddns-lab/ddns.git
cd ddns

# Build with all features
cargo build --release --bin ddnsd --features all

# Run directly
./target/release/ddnsd --version
```

## Usage Examples

### Example 1: Update IPv4 A Record

```bash
export DDNS_IP_SOURCE_TYPE=netlink
export DDNS_IP_SOURCE_VERSION=v4
export DDNS_PROVIDER_TYPE=cloudflare
export DDNS_PROVIDER_API_TOKEN=your_token
export DDNS_RECORDS=example.com:A

ddnsd
```

### Example 2: Update Both IPv4 and IPv6

```bash
export DDNS_IP_SOURCE_TYPE=netlink
export DDNS_IP_SOURCE_VERSION=both
export DDNS_PROVIDER_TYPE=cloudflare
export DDNS_PROVIDER_API_TOKEN=your_token
export DDNS_RECORDS=example.com:A,example.com:AAAA

ddnsd
```

### Example 3: HTTP Polling (Fallback for Non-Linux)

```bash
export DDNS_IP_SOURCE_TYPE=http
export DDNS_IP_SOURCE_URL=https://icanhazip.com
export DDNS_IP_SOURCE_INTERVAL=300  # 5 minutes
export DDNS_PROVIDER_TYPE=cloudflare
export DDNS_PROVIDER_API_TOKEN=your_token
export DDNS_RECORDS=ddns.example.com

ddnsd
```

## Development

```bash
# Build all crates
cargo build

# Build with optimizations
cargo build --release

# Run tests
cargo test

# Run tests for specific crate
cargo test -p ddns-core
cargo test -p ddns-provider-cloudflare

# Format code
cargo fmt

# Run linter
cargo clippy

# Check without building
cargo check
```

## Adding New Providers

To add a new DNS provider:

1. Create new crate: `crates/ddns-provider-{name}/`
2. Implement `DnsProvider` trait from `ddns-core`
3. Implement `DnsProviderFactory` for config-based creation
4. Export `register()` function to register with `ProviderRegistry`
5. Add as optional dependency to `ddnsd/Cargo.toml`
6. Add feature flag in `ddnsd/Cargo.toml`

See [`ddns-provider-cloudflare`](crates/ddns-provider-cloudflare/) as a reference implementation.

## License

Apache License 2.0

## Contributing

Please read [`.ai/AI_CONTRACT.md`](.ai/AI_CONTRACT.md) before contributing. This project has strict architectural constraints that must be followed.

### CI/CD Status

This project uses GitHub Actions for continuous integration and deployment:

- ✅ **CI**: Tests, formatting checks, and linting on every push and PR
- ✅ **Security Audit**: Automated dependency vulnerability scanning
- ✅ **Releases**: Automated multi-platform builds with GitHub release notes
- ✅ **Coverage**: Code coverage tracking (Codecov)

[![CI](https://img.shields.io/github/actions/workflow/status/ddns-lab/ddns/ci.yml?branch=main)](https://github.com/ddns-lab/ddns/actions/workflows/ci.yml)

All checks must pass before code can be merged into main.

## Changelog

### v0.1.1 (2025-01-14)
- ✨ **Auto-create DNS records**: Automatically creates missing DNS records
- 🐛 **Fix**: Cloudflare API 400 error (missing 'name' field in PUT request)
- 🐛 **Fix**: Daemon crash after 30 seconds (removed timeout on shutdown wait)
- 🐛 **Fix**: Initial DNS update not triggered on startup
- 📝 **Docs**: Automatic release notes generation from commit history
- 🧪 **Tests**: Fixed architectural contract tests with `run_for_testing()`

### v0.1.0 (2025-01-13)
- 🎉 **Initial release**: Event-driven DDNS system with:
  - Linux Netlink IP source (RTM_NEWADDR/RTM_DELADDR)
  - HTTP polling IP source (fallback for non-Linux)
  - Cloudflare DNS provider with A/AAAA support
  - File & Memory state stores with atomic writes
  - Idempotency via state tracking
  - Retry logic with exponential backoff
  - Environment variable configuration
  - Systemd integration with installer script
