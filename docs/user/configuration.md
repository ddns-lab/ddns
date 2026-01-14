# ⚙️ Configuration

Complete reference for all ddnsd environment variables.

---

## 📋 Quick Reference

**Minimum required configuration** (3 variables):

```bash
DDNS_PROVIDER_API_TOKEN=your_cloudflare_token
DDNS_RECORDS=example.com
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

**Configuration file location**: `/etc/ddnsd/ddnsd.env`

**Important**:
- ✅ All configuration via environment variables only
- ✅ Loaded once at startup
- ❌ No config files (TOML/YAML)
- ❌ No hot-reload (restart daemon to apply changes)

---

## 🔑 Required Variables

These variables **must** be set for ddnsd to start:

| Variable | Description | Example | Required |
|----------|-------------|---------|----------|
| `DDNS_PROVIDER_API_TOKEN` | Cloudflare API token | `d1234abc...` | ✅ Yes |
| `DDNS_RECORDS` | DNS records to update (comma-separated) | `example.com,www.example.com` | ✅ Yes |

### DDNS_PROVIDER_API_TOKEN

**Cloudflare API Token** with minimum permissions:
- Zone - DNS - Edit
- Zone - Zone - Read

**How to create**:
1. Go to Cloudflare Dashboard → My Profile → API Tokens
2. Create Token → Use template "Edit zone DNS"
3. Limit to specific zones (recommended)
4. Copy token

**Token format**: 40-character string starting with `d`

### DDNS_RECORDS

**Format**: `name[:type]` where `type` is `A`, `AAAA`, or omitted (auto-detect)

**Examples**:
```bash
# Auto-detect IP type (recommended)
DDNS_RECORDS=example.com

# Explicit IPv4 A record
DDNS_RECORDS=example.com:A

# Explicit IPv6 AAAA record
DDNS_RECORDS=example.com:AAAA

# Multiple records
DDNS_RECORDS=example.com:A,www.example.com:AAAA,api.example.com
```

**Feature**: Records are **auto-created** if they don't exist in Cloudflare.

---

## 📡 IP Source Configuration

**Controls how ddnsd detects IP address changes**

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_IP_SOURCE_TYPE` | IP detection method | `netlink` (Linux) | `netlink`, `http` |
| `DDNS_IP_SOURCE_INTERFACE` | Network interface to monitor | All interfaces | `eth0`, `wlan0`, etc. |
| `DDNS_IP_SOURCE_URL` | URL to fetch IP from (http mode) | None | Any HTTP(S) URL |
| `DDNS_IP_SOURCE_INTERVAL` | Poll interval in seconds (http mode) | `60` | `1` to `86400` |

### IP Source Types

**netlink** (recommended for Linux):
- ✅ Event-driven (instant notification)
- ✅ Zero CPU usage when idle
- ✅ No polling overhead
- ❌ Linux-only

```bash
DDNS_IP_SOURCE_TYPE=netlink
DDNS_IP_SOURCE_INTERFACE=eth0  # Optional: monitor specific interface
```

**http** (fallback for non-Linux):
- ✅ Cross-platform
- ❌ Polling-based (not instant)
- ❌ Higher CPU usage
- ❌ Requires external service

```bash
DDNS_IP_SOURCE_TYPE=http
DDNS_IP_SOURCE_URL=https://icanhazip.com
DDNS_IP_SOURCE_INTERVAL=300  # Check every 5 minutes
```

---

## 🌐 DNS Provider Configuration

**Controls which DNS provider and how to authenticate**

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_PROVIDER_TYPE` | DNS provider | `cloudflare` | `cloudflare` (currently only option) |
| `DDNS_PROVIDER_ZONE_ID` | Cloudflare Zone ID (optional) | Auto-detect | Cloudflare zone ID |

### DDNS_PROVIDER_ZONE_ID

**Optional**: Zone ID for faster API calls

**How to find**:
1. Cloudflare Dashboard → Select zone
2. Overview → Scroll down to "Zone ID"
3. Click to copy

**When to use**:
- Skip if you want auto-detection (simpler)
- Set if you have multiple zones with same domain

---

## 💾 State Store Configuration

**Controls where ddnsd stores state (for idempotency and crash recovery)**

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_STATE_STORE_TYPE` | State storage backend | `file` | `file`, `memory` |
| `DDNS_STATE_STORE_PATH` | Path to state file | None (required) | Any file path |

### State Store Types

**file** (recommended):
- ✅ Persists across restarts
- ✅ Crash recovery support
- ✅ Idempotency (prevents duplicate API calls)
- ❌ Requires disk I/O

```bash
DDNS_STATE_STORE_TYPE=file
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

**memory** (testing only):
- ✅ Fastest
- ❌ Lost on restart
- ❌ No crash recovery
- ⚠️ For testing only

```bash
DDNS_STATE_STORE_TYPE=memory
# No DDNS_STATE_STORE_PATH needed
```

---

## ⚙️ Engine Configuration

**Controls retry logic and update behavior**

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_MAX_RETRIES` | Maximum retry attempts for failed API calls | `3` | `0` to `10` |
| `DDNS_RETRY_DELAY_SECS` | Delay between retries | `5` | `0` to `3600` |
| `DDNS_MIN_UPDATE_INTERVAL_SECS` | Minimum time between DNS updates | `60` | `0` to `86400` |
| `DDNS_STARTUP_DELAY_SECS` | Initial startup delay | `0` | `0` to `60` |

### Retry Behavior

**Example**: With default settings (`DDNS_MAX_RETRIES=3`, `DDNS_RETRY_DELAY_SECS=5`)

```
API call fails
→ Wait 5 seconds
→ Retry 1
→ Wait 5 seconds
→ Retry 2
→ Wait 5 seconds
→ Retry 3
→ Give up (log error)
```

---

## 📝 Logging Configuration

**Controls log verbosity**

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_LOG_LEVEL` | Logging verbosity | `info` | `trace`, `debug`, `info`, `warn`, `error` |

### Log Levels

- **trace**: Extremely verbose (every internal step)
- **debug**: Detailed debugging (IP changes, API calls)
- **info**: Normal operation (startup, updates, errors)
- **warn**: Warning messages (API failures, retries)
- **error**: Errors only (failures)

**Viewing logs**:
```bash
# Follow logs in real-time
sudo journalctl -u ddnsd -f

# View last 50 lines
sudo journalctl -u ddnsd -n 50

# View errors only
sudo journalctl -u ddnsd -p err
```

---

## 🎯 Common Configuration Scenarios

### Scenario 1: Simple Home Server

**Use case**: Update single domain on home internet

```bash
# /etc/ddnsd/ddnsd.env
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=home.example.com
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

**How it works**:
- Uses netlink (event-driven, instant)
- Auto-detects IPv4 and IPv6
- Auto-creates record if missing

### Scenario 2: IPv4 Only

**Use case**: IPv4-only network or IPv6 not needed

```bash
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=home.example.com:A
DDNS_IP_SOURCE_TYPE=netlink
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

### Scenario 3: IPv6 Only

**Use case**: IPv6-only network

```bash
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=home.example.com:AAAA
DDNS_IP_SOURCE_TYPE=netlink
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

### Scenario 4: Multiple Records

**Use case**: Multiple subdomains on same server

```bash
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=example.com,www.example.com,api.example.com,matrix.example.com
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

### Scenario 5: HTTP Polling (non-Linux)

**Use case**: Running on macOS or Windows (with HTTP IP source)

```bash
DDNS_IP_SOURCE_TYPE=http
DDNS_IP_SOURCE_URL=https://icanhazip.com
DDNS_IP_SOURCE_INTERVAL=300
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=example.com
DDNS_STATE_STORE_PATH=/tmp/ddnsd-state.json
```

---

## 🔒 Security Best Practices

### API Token Security

```bash
# ✅ Good: File with restricted permissions
chmod 600 /etc/ddnsd/ddnsd.env

# ❌ Bad: Token in logs or version control
```

**Never commit** `/etc/ddnsd/ddnsd.env` to version control.

**Token rotation**: See [Secret Rotation Guide](../operations/secret-rotation.md)

### File Permissions

```bash
# State file permissions
sudo chmod 600 /var/lib/ddnsd/state.json
sudo chown root:root /var/lib/ddnsd/state.json

# Config directory
sudo chmod 755 /etc/ddnsd
sudo chown root:root /etc/ddnsd
```

---

## ✅ Configuration Validation

**Before starting**, verify configuration:

```bash
# Test configuration (dry run)
ddnsd --dry-run --once

# Check if configuration file is valid
source /etc/ddnsd/ddnsd.env
ddnsd --help
```

**Common validation errors**:

| Error | Cause | Solution |
|-------|-------|----------|
| `DDNS_PROVIDER_API_TOKEN is required` | API token not set | Add to config file |
| `DDNS_RECORDS must contain at least one record` | No records configured | Add at least one record |
| `DDNS_STATE_STORE_PATH is required` | File state store without path | Set state file path |
| `Failed to create netlink socket` | Netlink not available (non-Linux) | Use `DDNS_IP_SOURCE_TYPE=http` |

---

## 🔄 Applying Configuration Changes

**Configuration is loaded once at startup**. To apply changes:

```bash
# 1. Edit configuration
sudo vi /etc/ddnsd/ddnsd.env

# 2. Restart daemon
sudo systemctl restart ddnsd

# 3. Verify
sudo systemctl status ddnsd
sudo journalctl -u ddnsd -n 20
```

---

## 🎯 Next Steps

- **Deploy**: See [Deployment Guide](deployment.md)
- **Troubleshoot**: See [Troubleshooting Guide](troubleshooting.md)
- **Operations**: See [Operations Guide](../operations/)

---

## 🔗 Related Documentation

- [Installation](installation.md) - Installing ddnsd
- [Deployment](deployment.md) - systemd deployment
- [Troubleshooting](troubleshooting.md) - Common configuration issues
- [Security](../security/security.md) - Security best practices
