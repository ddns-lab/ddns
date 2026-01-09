# Operations Guide (OPS)

This document provides operational guidance for running and monitoring the ddns daemon in production environments (systemd, Docker, Kubernetes, etc.).

## Table of Contents

1. [Exit Semantics](#exit-semantics)
2. [Signal Handling](#signal-handling)
3. [Process Lifecycle](#process-lifecycle)
4. [Error Classification](#error-classification)
5. [Monitoring & Observability](#monitoring--observability)
6. [Deployment Patterns](#deployment-patterns)

---

## Exit Semantics

The ddns daemon uses explicit exit codes following systemd conventions to communicate termination reasons to supervisors and monitoring systems.

### Exit Codes

| Exit Code | Name | Meaning | Supervisor Action |
|-----------|------|---------|-------------------|
| **0** | CleanShutdown | Normal termination via signal | No action needed |
| **1** | ConfigError | Configuration validation failed | Do NOT restart (fix config first) |
| **2** | RuntimeError | Unexpected runtime error | Restart with backoff |

### Exit Code Usage

#### Exit Code 0: Clean Shutdown

**When it occurs:**
- Daemon received SIGTERM (e.g., `systemctl stop ddnsd`)
- Daemon received SIGINT (Ctrl+C)
- Daemon shut down within shutdown timeout (30 seconds)

**Supervisor behavior:**
- **Do NOT restart** - this is intentional termination
- Mark service as stopped
- Example: `systemd` treats this as successful stop

**Example:**
```bash
$ systemctl stop ddnsd
$ echo $?
0
```

---

#### Exit Code 1: Configuration Error

**When it occurs:**
- Required environment variable missing (e.g., `DDNS_PROVIDER_API_TOKEN`)
- Invalid configuration value (e.g., `DDNS_RECORDS` is empty)
- State store path required but not provided
- Tracing subsystem initialization failed

**Supervisor behavior:**
- **Do NOT restart automatically** - configuration is broken
- Restarting will fail again with same error
- Operator must fix configuration first

**Example:**
```bash
$ export DDNS_PROVIDER_API_TOKEN=""
$ ddnsd
Configuration error: DDNS_PROVIDER_API_TOKEN is required.
$ echo $?
1
```

**systemd unit configuration:**
```ini
[Service]
Restart=no
# Exit code 1 should not trigger restart
```

---

#### Exit Code 2: Runtime Error

**When it occurs:**
- Tokio runtime creation failed (unexpected)
- Daemon failed to initialize after successful config validation
- Shutdown timeout exceeded (daemon hung during shutdown)

**Supervisor behavior:**
- **Restart with exponential backoff**
- This indicates an unexpected failure that may be transient
- Monitor restart frequency; if too frequent, investigate logs

**Example:**
```bash
$ ddnsd
Starting ddnsd daemon
Configuration loaded: 2 record(s)
Daemon error: Failed to create tokio runtime: ...
$ echo $?
2
```

**systemd unit configuration:**
```ini
[Service]
Restart=on-failure
RestartSec=5s
# Exit code 2 triggers restart
```

---

## Signal Handling

The ddns daemon handles Unix signals to enable graceful shutdown in production environments.

### Supported Signals

| Signal | Unix | Windows | Behavior |
|--------|------|---------|----------|
| SIGTERM | ✅ | ❌ | Graceful shutdown (systemd stop) |
| SIGINT | ✅ | ✅ | Graceful shutdown (Ctrl+C) |
| SIGKILL | ✅ | ✅ | Immediate termination (not handled) |

### Signal Handling Behavior

#### SIGTERM (Unix)

**Purpose**: Standard termination signal from systemd, Docker, Kubernetes

**Behavior**:
1. Daemon logs `"Received shutdown signal: SIGTERM"`
2. Daemon initiates graceful shutdown
3. Daemon has 30 seconds to complete cleanup
4. If shutdown completes → Exit code 0 (CleanShutdown)
5. If timeout exceeded → Exit code 2 (RuntimeError)

**Example:**
```bash
$ systemctl stop ddnsd
# Logs:
# ddnsd[1234]: Received shutdown signal: SIGTERM
# ddnsd[1234]: Shutting down daemon
```

---

#### SIGINT (Unix/Windows)

**Purpose**: Interactive termination (Ctrl+C in terminal)

**Behavior**: Same as SIGTERM

**Example:**
```bash
$ ddnsd
Starting ddnsd daemon
Ready to monitor IP changes
^CReceived shutdown signal: SIGINT
Shutting down daemon
```

---

#### SIGKILL

**Purpose**: Force termination by supervisor (not handled by daemon)

**Behavior**:
- Daemon receives no opportunity to clean up
- Immediate process termination
- Exit code may be 137 (128 + 9) or similar
- Supervisor may send SIGKILL if SIGTERM timeout exceeds configured limit

**Example (systemd):**
```ini
[Service]
TimeoutStopSec=90s
# If daemon doesn't stop in 90s, systemd sends SIGKILL
```

---

### Shutdown Timeout

**Default timeout**: 30 seconds

**Purpose**: Prevent daemon from hanging indefinitely during shutdown

**Behavior:**
- Daemon has 30 seconds from signal receipt to exit cleanly
- If timeout exceeded, daemon exits with code 2 (RuntimeError)
- Supervisor may then send SIGKILL

**Example:**
```rust
// In run_daemon():
let shutdown_result = wait_for_shutdown_with_timeout(Duration::from_secs(30)).await;

match shutdown_result {
    Ok(signal) => {
        info!("Received shutdown signal: {}", signal);
        info!("Shutting down daemon");
        // Perform cleanup...
        Ok(())  // Exit code 0
    }
    Err(e) => {
        error!("Shutdown error: {}", e);
        Err(e)  // Exit code 2
    }
}
```

---

## Process Lifecycle

The ddns daemon follows a strict lifecycle with explicit failure modes.

### Startup Phase

```
1. Load config from environment
   ├─ On error → Exit code 1 (ConfigError)
2. Validate config
   ├─ On error → Exit code 1 (ConfigError)
3. Initialize tracing
   ├─ On error → Exit code 1 (ConfigError)
4. Create tokio runtime
   ├─ On error → Exit code 2 (RuntimeError)
5. Initialize daemon (registry, providers, engine)
   ├─ On error → Exit code 2 (RuntimeError)
6. Enter signal wait loop
```

**Key principle**: All configuration errors fail immediately with exit code 1 before any async operations begin.

---

### Runtime Phase

```
Daemon is running and monitoring IP changes

For each IP change event:
1. Detect IP change via IpSource
2. Check state store for idempotency
3. If changed, call DnsProvider::update_record()
4. On success → update state store
5. On transient error → log and continue (DO NOT exit)
6. On unexpected error → log and continue (DO NOT exit)
```

**Key principle**: Transient errors during runtime DO NOT crash the process. The daemon continues running and logs errors.

---

### Shutdown Phase

```
1. Receive SIGTERM or SIGINT
2. Log signal received
3. Initiate graceful shutdown (max 30 seconds)
4. Cleanup resources (engine, providers, state store)
5. Exit with code 0 (CleanShutdown)
```

**Key principle**: Graceful shutdown completes within 30 seconds. If exceeded, exits with code 2 (RuntimeError).

---

## Error Classification

Errors are classified into two categories with distinct behaviors:

### Configuration Errors (Exit Code 1)

**Characteristics:**
- Detectable before runtime starts
- Require manual intervention to fix
- Restarting without fixing will always fail

**Examples:**
- Missing required environment variables
- Invalid configuration values
- Missing state file path when using file store

**Corrective action:**
1. Check logs for specific error message
2. Fix configuration (set env vars, create directories)
3. Restart daemon manually

**Supervisor configuration:**
```ini
[Service]
Restart=no
# Exit code 1 = config error, do not restart
```

---

### Runtime Errors (Exit Code 2)

**Characteristics:**
- Occur after successful configuration
- May be transient (e.g., temporary network failure)
- Restarting may resolve the issue

**Examples:**
- Tokio runtime creation failure (rare)
- Daemon initialization failure after config load
- Shutdown timeout exceeded

**Corrective action:**
1. Check logs for error details
2. If transient issue (e.g., network), allow restart
3. If persistent issue (e.g., resource limits), investigate environment

**Supervisor configuration:**
```ini
[Service]
Restart=on-failure
RestartSec=5s
# Exit code 2 = runtime error, restart with backoff
```

---

### Transient Errors (No Exit)

**Characteristics:**
- Occur during normal operation
- Do NOT cause process exit
- Logged and handled internally

**Examples:**
- DNS API call fails (rate limit, network error)
- IP source temporary failure
- State store write failure (with retry)

**Corrective action:**
- Monitor logs for error patterns
- Daemon automatically retries (with backoff)
- No manual intervention needed unless errors persist

---

## Monitoring & Observability

### Log Levels

The daemon supports configurable log levels via `DDNS_LOG_LEVEL`:

| Level | Use Case | Example Messages |
|-------|----------|------------------|
| **trace** | Development debugging | Detailed event flow |
| **debug** | Troubleshooting | Event details, state transitions |
| **info** | Normal operation | Startup, shutdown, IP changes |
| **warn** | Abnormal but recoverable | Retry attempts, API rate limits |
| **error** | Failure requiring attention | API failures, state store errors |

**Default**: `info`

**Example:**
```bash
export DDNS_LOG_LEVEL=debug
ddnsd
```

---

### Key Log Messages

#### Startup
```
INFO Starting ddnsd daemon
INFO Configuration loaded: 2 record(s)
INFO IP source type: netlink
INFO Provider type: cloudflare
INFO State store type: file
INFO Managing record: example.com
INFO Managing record: www.example.com
INFO Daemon initialized successfully
INFO Ready to monitor IP changes
```

#### Shutdown (Normal)
```
INFO Received shutdown signal: SIGTERM
INFO Shutting down daemon
```

#### Shutdown (Timeout)
```
ERROR Shutdown error: Shutdown timeout after 30s
```

#### Configuration Error
```
ERROR Configuration validation error: DDNS_PROVIDER_API_TOKEN is required.
```

#### Runtime Error
```
ERROR Daemon error: Failed to create tokio runtime: ...
```

---

### Metrics to Monitor

1. **Exit codes**: Monitor for frequent exit code 2 (restarts)
2. **DNS update frequency**: Should be minimal (only when IP changes)
3. **API rate limits**: Monitor provider-specific rate limit errors
4. **Shutdown duration**: Should complete within 30 seconds
5. **State file writes**: Should occur on every successful DNS update

---

## Deployment Patterns

### systemd Service

**Unit file**: `/etc/systemd/system/ddnsd.service`

```ini
[Unit]
Description=DDNS Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
User=ddns
Group=ddns
EnvironmentFile=/etc/default/ddnsd

# Restart policy
Restart=on-failure
RestartSec=5s

# Shutdown timeout (must be >= daemon's 30s timeout)
TimeoutStopSec=90s

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/ddns

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=ddnsd

[Install]
WantedBy=multi-user.target
```

**Environment file**: `/etc/default/ddnsd`

```bash
DDNS_IP_SOURCE_TYPE=netlink
DDNS_IP_SOURCE_INTERFACE=eth0
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=your_token_here
DDNS_RECORDS=example.com,www.example.com
DDNS_STATE_STORE_TYPE=file
DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
DDNS_LOG_LEVEL=info
```

**Commands:**
```bash
# Enable and start
systemctl enable ddnsd
systemctl start ddnsd

# Check status
systemctl status ddnsd

# View logs
journalctl -u ddnsd -f

# Stop
systemctl stop ddnsd

# Restart after config change
systemctl edit ddnsd  # Edit environment
systemctl restart ddnsd
```

---

### Docker Container

**Dockerfile**:
```dockerfile
FROM rust:1.83-alpine AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin ddnsd

FROM alpine:3.19
RUN addgroup -S ddns && adduser -S ddns -G ddns
COPY --from=builder /build/target/release/ddnsd /usr/local/bin/
RUN chmod +x /usr/local/bin/ddnsd
USER ddns
ENTRYPOINT ["ddnsd"]
```

**Run**:
```bash
docker run -d \
  --name ddnsd \
  --network host \
  --restart on-failure \
  -e DDNS_IP_SOURCE_TYPE=netlink \
  -e DDNS_IP_SOURCE_INTERFACE=eth0 \
  -e DDNS_PROVIDER_TYPE=cloudflare \
  -e DDNS_PROVIDER_API_TOKEN=your_token \
  -e DDNS_RECORDS=example.com \
  -e DDNS_STATE_STORE_TYPE=memory \
  ddnsd:latest
```

**Graceful shutdown**:
```bash
docker stop ddnsd  # Sends SIGTERM, waits 10s
```

---

### Kubernetes Deployment

**Deployment**: `ddnsd-deployment.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ddnsd
spec:
  replicas: 1
  selector:
    matchLabels:
      app: ddnsd
  template:
    metadata:
      labels:
        app: ddnsd
    spec:
      containers:
      - name: ddnsd
        image: ddnsd:latest
        env:
        - name: DDNS_IP_SOURCE_TYPE
          value: "netlink"
        - name: DDNS_IP_SOURCE_INTERFACE
          value: "eth0"
        - name: DDNS_PROVIDER_TYPE
          value: "cloudflare"
        - name: DDNS_PROVIDER_API_TOKEN
          valueFrom:
            secretKeyRef:
              name: ddnsd-secrets
              key: api-token
        - name: DDNS_RECORDS
          value: "example.com"
        - name: DDNS_STATE_STORE_TYPE
          value: "memory"
        resources:
          requests:
            memory: "32Mi"
            cpu: "100m"
          limits:
            memory: "64Mi"
            cpu: "200m"
        terminationMessagePath: /dev/termination-log
        terminationMessagePolicy: FallbackToLogsOnError
```

**Graceful shutdown**: Kubernetes sends SIGTERM, waits for `terminationGracePeriodSeconds` (default: 30s), then sends SIGKILL.

---

## Troubleshooting

### Daemon exits immediately with code 1

**Symptom**: `systemctl start ddnsd` fails immediately

**Diagnosis**:
```bash
journalctl -u ddnsd -n 50
# Look for: "Configuration error: ..."
```

**Common causes**:
- Missing `DDNS_PROVIDER_API_TOKEN`
- Empty `DDNS_RECORDS`
- Missing `DDNS_STATE_STORE_PATH` when using file store

**Fix**: Correct configuration, then restart:
```bash
systemctl edit ddnsd  # Fix env vars
systemctl start ddnsd
```

---

### Daemon repeatedly restarts (code 2)

**Symptom**: `systemctl status ddnsd` shows frequent restarts

**Diagnosis**:
```bash
journalctl -u ddnsd -n 100 --no-pager
# Look for: "Daemon error: ..." or "Shutdown error: ..."
```

**Common causes**:
- Tokio runtime failure (resource limits)
- Shutdown timeout (daemon hanging)
- State file permission errors

**Fix**: Investigate logs, fix underlying issue, then restart:
```bash
# Check resource limits
ulimit -a

# Check file permissions
ls -la /var/lib/ddns/

# Restart
systemctl restart ddnsd
```

---

### Daemon not responding to SIGTERM

**Symptom**: `systemctl stop ddnsd` hangs, then sends SIGKILL

**Diagnosis**:
```bash
journalctl -u ddnsd -f
# Look for: "Received shutdown signal: SIGTERM"
# Then check if "Shutting down daemon" appears
```

**Common causes**:
- Engine shutdown hanging
- Long-running DNS update in progress
- State store write blocked

**Fix**:
1. Check if daemon is shutting down: `"Shutting down daemon"` in logs
2. If not, check for deadlocks or blocking operations
3. If shutdown timeout exceeded, increase timeout:
   ```ini
   [Service]
   TimeoutStopSec=120s  # Give more time for cleanup
   ```

---

## Summary

**Exit Codes:**
- **0**: Clean shutdown (normal)
- **1**: Configuration error (do not restart)
- **2**: Runtime error (restart with backoff)

**Signals:**
- **SIGTERM/SIGINT**: Graceful shutdown (30s timeout)
- **SIGKILL**: Immediate termination (not handled)

**Error Handling:**
- Config errors: Fail fast with exit code 1
- Runtime errors: Log and continue (transient)
- Unexpected errors: Exit with code 2

**Monitoring:**
- Watch logs for error messages
- Track exit codes via supervisor
- Monitor restart frequency

**Deployment:**
- systemd: Use `Restart=on-failure` for code 2 only
- Docker: Use `--restart on-failure`
- Kubernetes: Default restart policy handles exit codes appropriately
