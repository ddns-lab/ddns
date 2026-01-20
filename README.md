# DDNS

[![CI](https://img.shields.io/github/actions/workflow/status/ddns-lab/ddns/ci.yml?branch=main&label=CI)](https://github.com/ddns-lab/ddns/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/ddns-lab/ddns/branch/main/graph/badge.svg)](https://codecov.io/gh/ddns-lab/ddns)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://www.rust-lang.org)
[![GitHub Release](https://img.shields.io/github/v/release/ddns-lab/ddns)](https://github.com/ddns-lab/ddns/releases/latest)
[![GitHub Issues](https://img.shields.io/github/issues/ddns-lab/ddns)](https://github.com/ddns-lab/ddns/issues)
[![GitHub Discussions](https://img.shields.io/github/discussions/ddns-lab/ddns)](https://github.com/ddns-lab/ddns/discussions)

An event-driven Dynamic DNS system built with Rust, designed for high performance and **minimal resource consumption**.

---

## 🚀 Quick Start

### One-Line Installation (Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

This will:
- Download the latest binary
- Install to `/usr/local/bin/ddnsd`
- Create systemd service with auto-start on boot
- Configure via `/etc/ddnsd/ddnsd.env`

### Quick Configuration

Edit `/etc/ddnsd/ddnsd.env`:

```bash
# Provider selection
DDNS_PROVIDER_TYPE=cloudflare
DDNS_CLOUDFLARE_API_TOKEN=your_api_token_here

# Records to update
DDNS_RECORDS=ddns.example.com

# IP source (Linux netlink - event driven)
DDNS_IP_SOURCE_TYPE=netlink
```

Start the service:

```bash
sudo systemctl start ddnsd
sudo systemctl enable ddnsd  # Auto-start on boot
sudo systemctl status ddnsd
```

---

## 📖 Documentation

### For Users

**Getting Started**:
- [Installation Guide](docs/user/installation.md) - Detailed installation options
- [Configuration Guide](docs/user/configuration.md) - Complete environment variable reference
- [Troubleshooting Guide](docs/user/troubleshooting.md) - Common issues and solutions

**Advanced**:
- [Deployment Guide](docs/user/deployment.md) - Systemd deployment and verification
- [Migration Guide](docs/user/migration.md) - Version upgrade instructions

### For Developers

**Architecture**:
- [Architecture Documentation](docs/architecture/) - System design and component boundaries
- [`.ai/AI_CONTRACT.md`](.ai/AI_CONTRACT.md) - **Mandatory** architectural constraints
- [`CLAUDE.md`](CLAUDE.md) - Comprehensive development guide

---

## ⚡ Performance

| Metric | Value | Comparison |
|--------|-------|-------------|
| **Binary Size** | **3.5 MB** | Smaller than a single HD photo |
| **Memory Usage** | **~13 MB RSS** | Fraction of Go/Python implementations (100-500MB+) |
| **Startup Time** | **~20 ms** | Near-instant, no JVM warmup |
| **CPU Idle** | **~0%** | Event-driven, no polling threads |

**Why so efficient?**

- ✅ **Zero-cost abstractions**: Rust's compile-time optimization
- ✅ **Event-driven architecture**: No polling, no background threads
- ✅ **No garbage collection**: Deterministic memory usage, no GC pauses
- ✅ **Minimal runtime**: No VM, no interpreter, bare metal performance

---

## 🌐 Supported DNS Providers

### Production Ready ✅

| Provider | Status | DNS Propagation |
|----------|--------|-----------------|
| **Cloudflare** | ✅ Production Ready | <5 seconds |
| **Aliyun** | ✅ Core Verified | ~20 seconds |
| **NameSilo** | ✅ Production Ready | >20 seconds |

### Code Ready 🟡

| Provider | Status | Notes |
|----------|--------|-------|
| **GoDaddy** | 🟡 Code Ready | Implementation verified, pending network test |

### Environment Variables

**Cloudflare**:
```bash
DDNS_CLOUDFLARE_API_TOKEN=your_token
DDNS_CLOUDFLARE_ZONE_ID=your_zone_id  # optional
```

**Aliyun**:
```bash
DDNS_ALIYUN_ACCESS_KEY_ID=your_key_id
DDNS_ALIYUN_ACCESS_KEY_SECRET=your_secret
```

**NameSilo**:
```bash
DDNS_NAMESILO_API_KEY=your_api_key
```

**GoDaddy**:
```bash
DDNS_GODADDY_API_KEY=your_key
DDNS_GODADDY_API_SECRET=your_secret
DDNS_GODADDY_OTE=true  # optional: use test environment
```

---

## 🧪 Testing

### Build from Source

```bash
git clone https://github.com/ddns-lab/ddns.git
cd ddns
cargo build --release --bin ddnsd --features all
```

### Run Tests

```bash
# Mock tests only (no network required)
cargo test

# All tests (Linux only - requires root)
sudo cargo test -- --ignored
```

---

## 📋 Requirements

### System Requirements

- **OS**: Linux (recommended), macOS, Windows
- **Memory**: ~13 MB RAM
- **Disk**: 3.5 MB binary size
- **Privileges**: None required for HTTP IP source
- **Privileges**: CAP_NET_ADMIN for netlink IP source (usually root/sudo)

### DNS Provider Account

You need an account with one of the supported providers:
- [Cloudflare](https://cloudflare.com) - Free tier available
- [Aliyun](https://aliyun.com) - Alibaba Cloud DNS
- [NameSilo](https://namesilo.com) - Budget DNS provider
- GoDaddy - Paid DNS service

---

## 🔧 Configuration

### Environment Variables

#### Core Settings

```bash
# Provider selection (required)
DDNS_PROVIDER_TYPE=cloudflare|aliyun|namesilo|godaddy

# Records to update (required, comma-separated)
DDNS_RECORDS=example.com,www.example.com

# IP source (default: netlink)
DDNS_IP_SOURCE_TYPE=netlink|http

# State storage (default: file)
DDNS_STATE_STORE_TYPE=file|memory

# Log level (default: info)
DDNS_LOG_LEVEL=trace|debug|info|warn|error
```

#### Provider-Specific Settings

See [Configuration Guide](docs/user/configuration.md) for complete reference.

---

## ❓ Troubleshooting

### Common Issues

**Problem**: `ddnsd: command not found`
- **Solution**: Binary not in PATH. Use full path: `/usr/local/bin/ddnsd`

**Problem**: `Permission denied` when creating netlink socket
- **Solution**: Run with sudo or add CAP_NET_ADMIN capability

**Problem**: DNS records not updating
- **Solution**: Check provider credentials and verify API token permissions

**Problem**: "Record not found" error
- **Solution**: Some providers auto-create records (Cloudflare, NameSilo), others require manual creation first

For more solutions, see [Troubleshooting Guide](docs/user/troubleshooting.md).

---

## 🏗️ Architecture

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

**Key Design Principles**:
- ✅ **Event-driven**: React to IP changes instantly via Linux Netlink
- ✅ **Idempotent**: Prevents unnecessary DNS updates via state tracking
- ✅ **Plugin architecture**: Easy to add new DNS providers
- ✅ **Zero-cost abstractions**: Rust safety without performance penalty

---

## 📚 More Documentation

**Full Documentation**: [docs/README.md](docs/README.md)

**Key Documents**:
- [Installation](docs/user/installation.md) - 3 ways to install
- [Configuration](docs/user/configuration.md) - Complete env var reference
- [Troubleshooting](docs/user/troubleshooting.md) - 10 common issues
- [Operations](docs/operations/ops.md) - Process lifecycle and signals
- [Security](docs/security/security.md) - Security best practices

---

## 🤝 Contributing

Please read [`.ai/AI_CONTRACT.md`](.ai/AI_CONTRACT.md) before contributing. This project has strict architectural constraints that must be followed.

### Development

```bash
# Clone repository
git clone https://github.com/ddns-lab/ddns.git
cd ddns

# Build
cargo build

# Run tests
cargo test

# Format code
cargo fmt

# Run linter
cargo clippy
```

### Adding New Providers

1. Create new crate: `crates/ddns-provider-{name}/`
2. Implement `DnsProvider` trait from `ddns-core`
3. Implement `DnsProviderFactory` for config-based creation
4. Export `register()` function to register with `ProviderRegistry`
5. Add as optional dependency to `ddnsd/Cargo.toml`
6. Add feature flag in `ddnsd/Cargo.toml`

---

## 📄 License

Apache License 2.0

---

## 🎯 Project Goals

- **Extreme resource efficiency**: Minimal overhead, 3.5MB binary, ~13MB RAM
- **Event-driven**: React to IP changes instantly via Linux Netlink (no polling)
- **Zero-cost abstractions**: Rust's safety without performance penalty
- **Long-term stability**: Clear architecture, well-defined boundaries
- **Library-first**: Core logic reusable as a Rust library
- **Production-ready**: Comprehensive validation and error handling

---

## 🌟 Features

- ✅ **Ultra-lightweight**: 3.5MB binary, ~13MB RAM, 20ms startup
- ✅ **Event-driven architecture**: React to network changes instantly via Linux Netlink
- ✅ **Idempotency**: Prevents unnecessary DNS updates via state tracking
- ✅ **Provider plugin system**: Easy to add new DNS providers
- ✅ **Multi-provider support**: Cloudflare, Aliyun, NameSilo, GoDaddy
- ✅ **Dry-run mode**: Safe testing without making actual changes
- ✅ **Comprehensive error handling**: Clear error messages for all failure scenarios
- ✅ **Security-first**: API tokens never logged, env var config only
- ✅ **CI/CD**: GitHub Actions for testing, security auditing, and multi-platform builds

---

**For complete documentation, see [docs/README.md](docs/README.md)**
