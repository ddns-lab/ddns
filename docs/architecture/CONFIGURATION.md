# Configuration Reference

This document defines all configuration options for the ddns system.

## Configuration Principles

Per `.ai/AI_CONTRACT.md` §6:
- ✅ All runtime configuration comes from **environment variables**
- ✅ Configuration is loaded **once at startup**
- ❌ **NO** config files (TOML, YAML, JSON, etc.)
- ❌ **NO** hot-reload mechanisms
- ❌ **NO** interactive configuration
- ❌ **NO** embedded configuration UIs

## Environment Variables

### Required Variables

These variables **must** be set for the daemon to run:

| Variable | Description | Example |
|----------|-------------|---------|
| `DDNS_PROVIDER_API_TOKEN` | API token for DNS provider | `d1234abc...` |
| `DDNS_RECORDS` | Comma-separated list of DNS records | `example.com,www.example.com` |

### Optional Variables

These variables have **defaults** if not set:

#### IP Source Configuration

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_IP_SOURCE_TYPE` | Type of IP source | `netlink` | `netlink`, `http` |
| `DDNS_IP_SOURCE_INTERFACE` | Network interface (netlink) | `None` (all interfaces) | `eth0`, `wlan0`, etc. |
| `DDNS_IP_SOURCE_URL` | URL to fetch IP from (http) | *None* | Any HTTP URL |
| `DDNS_IP_SOURCE_INTERVAL` | Poll interval in seconds (http) | `60` | Any positive integer |

**Notes**:
- `netlink` is Linux-specific (event-driven, preferred)
- `http` is cross-platform (polling-based, fallback)

#### DNS Provider Configuration

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_PROVIDER_TYPE` | DNS provider type | `cloudflare` | `cloudflare` |
| `DDNS_PROVIDER_ZONE_ID` | Zone ID (optional) | `None` (auto-detect) | Cloudflare zone ID |

#### State Store Configuration

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_STATE_STORE_TYPE` | Type of state store | `file` | `file`, `memory` |
| `DDNS_STATE_STORE_PATH` | Path to state file (file) | *None* (required for file) | Any file path |

**Notes**:
- `file` state store persists across restarts (recommended)
- `memory` state store is lost on restart (testing only)

#### Engine Configuration

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_MAX_RETRIES` | Maximum retry attempts | `3` | `0` (no retries) to `10` |
| `DDNS_RETRY_DELAY_SECS` | Delay between retries (seconds) | `5` | `0` (immediate) to `3600` |
| `DDNS_STARTUP_DELAY_SECS` | Initial startup delay (seconds) | `0` | `0` to `60` |

#### Logging Configuration

| Variable | Description | Default | Valid Values |
|----------|-------------|---------|--------------|
| `DDNS_LOG_LEVEL` | Logging verbosity | `info` | `trace`, `debug`, `info`, `warn`, `error` |

## Default Behavior

### What Happens If Variables Are Not Set

| Variable | Not Set Behavior | Can Run? |
|----------|-----------------|----------|
| `DDNS_PROVIDER_API_TOKEN` | *Required* - daemon fails to start | ❌ No |
| `DDNS_RECORDS` | *Required* - daemon fails to start | ❌ No |
| `DDNS_IP_SOURCE_TYPE` | Uses `netlink` (Linux) or fails (non-Linux) | ⚠️ Platform-dependent |
| `DDNS_PROVIDER_TYPE` | Uses `cloudflare` | ✅ Yes |
| `DDNS_STATE_STORE_TYPE` | Uses `file` (requires `DDNS_STATE_STORE_PATH`) | ⚠️ See below |
| `DDNS_STATE_STORE_PATH` | Required if `DDNS_STATE_STORE_TYPE=file` | ⚠️ See below |
| `DDNS_MAX_RETRIES` | Uses `3` | ✅ Yes |
| `DDNS_RETRY_DELAY_SECS` | Uses `5` | ✅ Yes |
| `DDNS_LOG_LEVEL` | Uses `info` | ✅ Yes |

### Platform-Specific Defaults

**Linux**:
- `DDNS_IP_SOURCE_TYPE` → `netlink` (event-driven, preferred)
- `DDNS_STATE_STORE_TYPE` → `file` (requires `DDNS_STATE_STORE_PATH`)

**Non-Linux** (macOS, Windows, BSD):
- `DDNS_IP_SOURCE_TYPE` → Must be set to `http` (or daemon fails)
- `DDNS_STATE_STORE_TYPE` → `file` (requires `DDNS_STATE_STORE_PATH`)

## Configuration Loading

### Load Once at Startup

Configuration is loaded **exactly once** when the daemon starts:

```text
1. Parse environment variables
2. Validate configuration
3. If validation fails: Exit with error message
4. If validation succeeds: Start engine
5. Configuration is immutable after startup
```

### No Runtime Mutation

Once loaded, configuration **cannot be changed**:
- ❌ No hot-reload
- ❌ No SIGHUP handler
- ❌ No config reload on file change
- ✅ Must restart daemon to change configuration

This is **intentional** - simplifies reasoning about behavior.

## Configuration Validation

### Validation Rules

1. **Required fields must be set**
   - `DDNS_PROVIDER_API_TOKEN` cannot be empty
   - `DDNS_RECORDS` must contain at least one record

2. **Conditional requirements**
   - If `DDNS_STATE_STORE_TYPE=file`, then `DDNS_STATE_STORE_PATH` is required
   - If `DDNS_IP_SOURCE_TYPE=http`, then `DDNS_IP_SOURCE_URL` is required

3. **Type validation**
   - Numeric values must parse successfully
   - Invalid values cause startup to fail

### Error Messages

All validation errors include:
- What variable is missing/invalid
- Why it's required
- How to fix it (example command)

Example:
```
Error: DDNS_RECORDS must contain at least one record.
       Set it via: export DDNS_RECORDS=example.com,www.example.com
```

## Minimal Configuration Example

### Linux (netlink + file)

```bash
export DDNS_PROVIDER_API_TOKEN=your_cloudflare_token
export DDNS_RECORDS=example.com
export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json

ddnsd
```

This works because:
- `DDNS_IP_SOURCE_TYPE` → defaults to `netlink` (Linux)
- `DDNS_PROVIDER_TYPE` → defaults to `cloudflare`
- `DDNS_STATE_STORE_TYPE` → defaults to `file`
- `DDNS_MAX_RETRIES` → defaults to `3`
- `DDNS_RETRY_DELAY_SECS` → defaults to `5`
- `DDNS_LOG_LEVEL` → defaults to `info`

### macOS/Windows (http + file)

```bash
export DDNS_IP_SOURCE_TYPE=http
export DDNS_IP_SOURCE_URL=https://ifconfig.me/ip
export DDNS_IP_SOURCE_INTERVAL=300
export DDNS_PROVIDER_API_TOKEN=your_cloudflare_token
export DDNS_RECORDS=example.com
export DDNS_STATE_STORE_PATH=/tmp/ddns-state.json

ddnsd
```

This explicitly sets:
- `DDNS_IP_SOURCE_TYPE` to `http` (netlink not available)
- `DDNS_IP_SOURCE_URL` to a public IP service
- `DDNS_IP_SOURCE_INTERVAL` to 5 minutes

## Complete Configuration Example

```bash
# IP Source (Linux netlink)
export DDNS_IP_SOURCE_TYPE=netlink
export DDNS_IP_SOURCE_INTERFACE=eth0

# DNS Provider (Cloudflare)
export DDNS_PROVIDER_TYPE=cloudflare
export DDNS_PROVIDER_API_TOKEN=d1234abc567890ef1234567890ef12345678
export DDNS_PROVIDER_ZONE_ID=abc123def456

# Records to manage
export DDNS_RECORDS=example.com,www.example.com,api.example.com

# State Store (file-based)
export DDNS_STATE_STORE_TYPE=file
export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json

# Engine Behavior
export DDNS_MAX_RETRIES=3
export DDNS_RETRY_DELAY_SECS=5

# Logging
export DDNS_LOG_LEVEL=info

# Run daemon
ddnsd
```

## Security Considerations

### API Token Security

- **Never commit** `DDNS_PROVIDER_API_TOKEN` to version control
- Use `.env` files (add to `.gitignore`)
- Use secret management systems in production
- Rotate tokens regularly

### File Permissions

State files may contain sensitive information:
- Set appropriate permissions: `chmod 600 /var/lib/ddns/state.json`
- Store in secure directory: `/var/lib/ddns/` with `root:ddns 750`
- Consider encryption for production deployments

## Configuration Surface Constraints

Per `.ai/AI_CONTRACT.md` §6, the following are **forbidden**:

### ❌ Configuration Files

Do NOT add:
- Config file support (TOML, YAML, JSON, INI, etc.)
- Config file discovery (`./config.toml`, `~/.ddns/config`, etc.)
- Config file validation or parsing

### ❌ Hot-Reload

Do NOT add:
- SIGHUP handler to reload config
- File watcher for config changes
- Runtime config mutation API

### ❌ Interactive Configuration

Do NOT add:
- `--init` flag to prompt for configuration
- `--configure` flag for interactive setup
- TTY prompts for missing values

### ❌ Embedded Configuration UIs

Do NOT add:
- Web UI for configuration
- Embedded HTTP server
- gRPC/REST API for config changes

## Configuration Changes Require Restart

To change configuration:
1. Stop the daemon: `kill $PID` or `Ctrl+C`
2. Update environment variables
3. Start the daemon: `ddnsd`

**This is intentional** - configuration is loaded once and immutable.

## Troubleshooting

### "DDNS_PROVIDER_API_TOKEN is required"

**Problem**: API token not set or empty

**Solution**:
```bash
export DDNS_PROVIDER_API_TOKEN=your_actual_token
ddnsd
```

### "DDNS_RECORDS must contain at least one record"

**Problem**: No records configured

**Solution**:
```bash
export DDNS_RECORDS=example.com
ddnsd
```

### "DDNS_STATE_STORE_PATH is required when DDNS_STATE_STORE_TYPE=file"

**Problem**: File state store selected but no path specified

**Solution**:
```bash
export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
ddnsd
```

### "Failed to create netlink socket" (non-Linux)

**Problem**: Netlink is Linux-only

**Solution**:
```bash
export DDNS_IP_SOURCE_TYPE=http
export DDNS_IP_SOURCE_URL=https://ifconfig.me/ip
ddnsd
```
