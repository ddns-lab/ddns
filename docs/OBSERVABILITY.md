# Operational Observability Contract

This document defines the observability requirements for the ddns daemon in production environments.

## Table of Contents

1. [Logging Contract](#logging-contract)
2. [Metrics Contract](#metrics-contract)
3. [Health Check Contract](#health-check-contract)
4. [Alerting Guidelines](#alerting-guidelines)
5. [Monitoring Integration](#monitoring-integration)

---

## Logging Contract

### Structured Logging Standard

The daemon uses **structured logging** via the `tracing` crate.

**Log levels** (in order of severity):

| Level | Purpose | Example |
|-------|---------|---------|
| **TRACE** | Extremely detailed diagnostic info | "Received event on channel" |
| **DEBUG** | Detailed information for debugging | "State loaded: 2 records" |
| **INFO** | Normal operational messages | "Starting ddnsd daemon" |
| **WARN** | Abnormal but recoverable situations | "Retrying DNS update" |
| **ERROR** | Error that doesn't crash process | "API call failed: timeout" |

**Default level**: `INFO` (configurable via `DDNS_LOG_LEVEL`)

---

### Required Log Messages

The daemon MUST log the following events:

#### Startup Sequence

**INFO level**:

```rust
// Configuration loaded
info!("Starting ddnsd daemon");
info!("Configuration loaded: {} record(s)", records.len());

// Component initialization
info!("IP source type: {}", ip_source_type);
info!("Provider type: {}", provider_type);
info!("State store type: {}", state_store_type);

// Per-record setup
for record in &records {
    info!("Managing record: {}", record);
}

// Ready state
info!("Daemon initialized successfully");
info!("Ready to monitor IP changes");
```

**Example output**:
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

---

#### Normal Operation

**INFO level - IP changes detected**:

```rust
info!("IP change detected: {} -> {}", old_ip, new_ip);
info!("Updating DNS record: {}", record_name);
info!("DNS update successful: {} -> {}", record_name, new_ip);
info!("State saved: {}", record_name);
```

**DEBUG level - State management**:

```rust
debug!("State loaded from file: {} records", count);
debug!("Checking idempotency: current={}, new={}", current_ip, new_ip);
debug!("Update needed: {}", record_name);
debug!("Update not needed: IP unchanged for {}", record_name);
```

---

#### Error Scenarios

**WARN level - Transient failures**:

```rust
// Retry attempts
warn!("DNS update failed (attempt {}/{}): {}", attempt, max_retries, error);
warn!("Retrying in {} seconds...", delay_secs);

// API rate limiting
warn!("API rate limit reached, backing off");
warn!("Provider returned 429 Too Many Requests");

// Network issues
warn!("Network timeout, retrying");
warn!("Connection refused, will retry");
```

**ERROR level - Failures**:

```rust
// State store errors
error!("Failed to save state: {}", error);
error!("Failed to load state: {}", error);

// Provider errors
error!("API authentication failed: {}", error);
error!("API call failed: {}", error);

// IP source errors
error!("Failed to detect IP: {}", error);
error!("IP source error: {}", error);
```

---

#### Shutdown Sequence

**INFO level**:

```rust
// Signal received
info!("Received shutdown signal: {}", signal);  // SIGTERM or SIGINT

// Shutdown initiated
info!("Shutting down daemon");

// Final state
info!("State saved to disk");
info!("Daemon stopped");
```

**Example output**:
```
INFO Received shutdown signal: SIGTERM
INFO Shutting down daemon
INFO State saved to disk
INFO Daemon stopped
```

---

### Prohibited Log Messages

**NEVER log** (security risk):

❌ API tokens
❌ Secret keys
❌ Sensitive configuration
❌ User data (unless DNS record names)

**Example of WRONG logging**:
```rust
// WRONG: Logs API token
error!("Authentication failed with token: {}", api_token);

// CORRECT: No token in log
error!("API authentication failed: invalid token");
```

---

### Log Format

**Default format** (tracing-subscriber FMT):

```
[2025-01-09T12:00:00.123Z INFO ddnsd]: Starting ddnsd daemon
[2025-01-09T12:00:00.456Z INFO ddnsd]: Configuration loaded: 2 record(s)
[2025-01-09T12:00:00.789Z WARN ddnsd]: Retrying DNS update (1/3): timeout
```

**Structured format** (via tracing-subscriber JSON):

```json
{"timestamp":"2025-01-09T12:00:00.123Z","level":"INFO","target":"ddnsd","message":"Starting ddnsd daemon"}
{"timestamp":"2025-01-09T12:00:00.456Z","level":"INFO","target":"ddnsd","message":"Configuration loaded","records":2}
```

**Enable JSON logging** (for log aggregators):
```bash
export DDNS_LOG_FORMAT=json
```

---

### Log Rotation

**systemd**: Automatic via journald
- Max size: 10MB per journal file
- Retention: Configurable via `SystemMaxUse`

**Docker**: Configure in logging section
```yaml
logging:
  driver: "json-file"
  options:
    max-size: "10m"
    max-file: "3"
```

**Kubernetes**: stdout/stderr captured by cluster logging
- No rotation needed (handled by cluster)
- Configure retention in logging stack (ELK, Loki, etc.)

---

## Metrics Contract

### Key Metrics

The daemon should track and expose the following metrics:

#### DNS Update Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ddns_updates_total` | Counter | Total number of DNS update attempts |
| `ddns_updates_successful_total` | Counter | Total successful DNS updates |
| `ddns_updates_failed_total` | Counter | Total failed DNS updates |
| `ddns_update_duration_seconds` | Histogram | DNS update duration in seconds |

**Example**:
```
# Successful update
ddns_updates_total{record="example.com",provider="cloudflare"} 1
ddns_updates_successful_total{record="example.com",provider="cloudflare"} 1
ddns_update_duration_seconds{record="example.com"} 0.523

# Failed update
ddns_updates_total{record="example.com",provider="cloudflare"} 1
ddns_updates_failed_total{record="example.com",provider="cloudflare",error="timeout"} 1
```

---

#### IP Change Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ddns_ip_changes_total` | Counter | Total IP changes detected |
| `ddns_ip_changes_detected_seconds` | Gauge | Time since last IP change |

**Example**:
```
ddns_ip_changes_total{interface="eth0"} 1
ddns_ip_changes_detected_seconds{interface="eth0"} 123.45
```

---

#### State Store Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ddns_state_load_errors_total` | Counter | State load failures |
| `ddns_state_save_errors_total` | Counter | State save failures |
| `ddns_state_records` | Gauge | Number of records in state |

**Example**:
```
ddns_state_records{type="file"} 2
ddns_state_load_errors_total{type="file"} 0
```

---

#### API Call Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ddns_api_calls_total` | Counter | Total API calls to provider |
| `ddns_api_errors_total` | Counter | Total API errors |
| `ddns_api_rate_limit_hits_total` | Counter | API rate limit encounters |

**Example**:
```
ddns_api_calls_total{provider="cloudflare",endpoint="update_dns"} 1
ddns_api_errors_total{provider="cloudflare",error="429"} 0
```

---

### Metric Format

**Prometheus text format** (future implementation):

```
# HELP ddns_updates_total Total number of DNS update attempts
# TYPE ddns_updates_total counter
ddns_updates_total{record="example.com",provider="cloudflare"} 1

# HELP ddns_update_duration_seconds DNS update duration in seconds
# TYPE ddns_update_duration_seconds histogram
ddns_update_duration_seconds_bucket{record="example.com",le="0.1"} 0
ddns_update_duration_seconds_bucket{record="example.com",le="0.5"} 1
ddns_update_duration_seconds_bucket{record="example.com",le="1.0"} 1
ddns_update_duration_seconds_bucket{record="example.com",le="+Inf"} 1
ddns_update_duration_seconds_sum{record="example.com"} 0.523
ddns_update_duration_seconds_count{record="example.com"} 1
```

---

### Current Status

**Phase 20 Note**: Metrics are not yet implemented. The daemon currently uses **logging as the primary observability mechanism**.

**Future implementation** (Phase 20+):
- Expose Prometheus metrics endpoint
- Use `prometheus-client` or `metrics` crate
- Optional HTTP endpoint (e.g., `:9090/metrics`)
- Disabled by default (opt-in for security)

---

## Health Check Contract

### Health Definition

The daemon is **healthy** when:

1. ✅ Process is running
2. ✅ Configuration is valid
3. ✅ State store is accessible
4. ✅ No critical errors in last 5 minutes

The daemon is **unhealthy** when:

1. ❌ Process is not running
2. ❌ Configuration errors preventing operation
3. ❌ State store inaccessible
4. ❌ Repeated critical errors (API failures)

---

### Platform-Specific Health Checks

#### systemd

**Status check**:
```bash
systemctl is-active ddnsd
# Output: active (healthy) or inactive/inactive (unhealthy)
```

**Health integration**:
```bash
# systemd monitors process health via Type=simple
# If daemon exits, systemd restarts it (Restart=on-failure)
```

---

#### Docker

**HEALTHCHECK instruction** (Dockerfile):
```dockerfile
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD pgrep ddnsd || exit 1
```

**Health status**:
```bash
docker inspect ddnsd --format='{{.State.Health.Status}}'
# Output: healthy or unhealthy
```

**Monitoring**:
```bash
docker ps --format "table {{.Names}}\t{{.Status}}"
```

---

#### Kubernetes

**Liveness probe** (is the process running?):
```yaml
livenessProbe:
  exec:
    command:
    - /bin/sh
    - -c
    - pgrep ddnsd
  initialDelaySeconds: 5
  periodSeconds: 30
  timeoutSeconds: 10
  failureThreshold: 3
```

**Readiness probe** (is the process ready to work?):
```yaml
readinessProbe:
  exec:
    command:
    - /bin/sh
    - -c
    - pgrep ddnsd
  initialDelaySeconds: 2
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 1
```

**Health status**:
```bash
kubectl get pods -l app=ddnsd -n ddns-system
# Output: NAME    READY   STATUS    RESTARTS   AGE
#         ddnsd-xxx   1/1     Running   0          1h
```

---

### Future HTTP Health Endpoint

**Planned implementation** (opt-in):

```rust
// GET /health
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "last_update": "2025-01-09T12:00:00Z",
  "records": 2,
  "errors_last_5min": 0
}

// GET /health/ready
{
  "ready": true
}

// GET /metrics (Prometheus)
# Prometheus text format metrics
```

**Security consideration**: Only expose on localhost or require authentication.

---

## Alerting Guidelines

### Alert Severity Levels

| Severity | When to Alert | Response Time |
|----------|---------------|---------------|
| **Critical** | Daemon not running | Immediate (15 min) |
| **High** | Repeated failures (10+ in 1 hour) | 1 hour |
| **Medium** | State store corruption | 4 hours |
| **Low** | Single transient failure | Next business day |
| **Info** | Normal shutdown | No alert |

---

### Critical Alerts

**Daemon not running** (Critical):

```bash
# Condition: Process not running
if ! systemctl is-active --quiet ddnsd; then
    alert "Daemon not running"
fi
```

**Response**:
1. Check logs: `sudo journalctl -u ddnsd -n 50`
2. Check status: `sudo systemctl status ddnsd`
3. Restart: `sudo systemctl start ddnsd`
4. Investigate root cause

---

**Configuration errors** (Critical):

```bash
# Condition: Exit code 1 (config error)
if journalctl -u ddnsd --since "1 hour ago" | grep -q "Configuration error"; then
    alert "Configuration error preventing startup"
fi
```

**Response**:
1. Check logs for specific error
2. Fix configuration (`/etc/default/ddnsd`)
3. Restart daemon

---

### High Severity Alerts

**Repeated DNS update failures** (High):

```bash
# Condition: 10+ failures in 1 hour
FAIL_COUNT=$(journalctl -u ddnsd --since "1 hour ago" | grep -c "DNS update failed")
if [ $FAIL_COUNT -gt 10 ]; then
    alert "High DNS update failure rate: $FAIL_COUNT failures in 1 hour"
fi
```

**Response**:
1. Check logs for error pattern
2. Verify API token is valid
3. Check provider API status
4. Verify network connectivity

---

**API rate limiting** (High):

```bash
# Condition: Rate limit errors
if journalctl -u ddnsd --since "1 hour ago" | grep -q "429\|rate limit"; then
    alert "API rate limit reached"
fi
```

**Response**:
1. Check update frequency (may be too high)
2. Verify `DDNS_MIN_UPDATE_INTERVAL_SECS` setting
3. Contact provider to increase quota if needed

---

### Medium Severity Alerts

**State store corruption** (Medium):

```bash
# Condition: Corruption detected
if journalctl -u ddnsd --since "24 hours" | grep -q "corrupted"; then
    alert "State file corruption detected and recovered"
fi
```

**Response**:
1. Verify recovery was successful
2. Check disk health
3. Monitor for recurrence

---

### Low Severity Alerts

**Single transient failure** (Low):

**No alert needed** - daemon retries automatically.

**Consider alerting if**: Pattern of repeated transient failures (escalates to High).

---

### Alert Silencing Rules

**Don't alert** during:

- Scheduled maintenance (maintenance windows)
- Known upgrades/rollouts
- System shutdown (SIGTERM/SIGINT received)

**Example** (maintenance window):
```bash
# Silence alerts during maintenance
# Add to alerting system:
# silence during: 02:00-04:00 daily
```

---

## Monitoring Integration

### Log Aggregation

**ELK Stack** (Elasticsearch, Logstash, Kibana):

```bash
# Filebeat configuration
filebeat.inputs:
- type: container
  paths:
    - '/var/lib/docker/containers/*ddnsd*.log'
  processors:
  - add_docker_metadata:
```

**Loki** (Grafana Loki):

```yaml
# Promtail configuration
scrape_configs:
- job_name: ddnsd
  static_configs:
  - targets:
      - localhost
    labels:
      job: ddnsd
      __path__: /var/log/journal/*ddnsd*.log
```

**Fluentd**:

```xml
# Fluentd configuration
<source>
  @type systemd
  <storage>
    @type local
    path /var/log/fluentd-journald-ddnsd.pos
  </storage>
  filters [{ "_SYSTEMD_UNIT": "ddnsd.service" }]
  tag ddnsd
</source>
```

---

### Metrics Collection

**Prometheus** (future implementation):

```yaml
# prometheus.yml
scrape_configs:
- job_name: 'ddnsd'
  static_configs:
  - targets: ['localhost:9090']
  scrape_interval: 15s
```

**StatsD** (alternative):

```bash
# Export metrics to StatsD
export DDNS_METRICS_BACKEND=statsd
export DDNS_STATSD_HOST=localhost:8125
```

---

### Dashboard Examples

**Grafana dashboard panels**:

1. **DNS Updates Rate**:
   - Query: `rate(ddns_updates_total[5m])`
   - Display: Time series graph

2. **Update Success Rate**:
   - Query: `rate(ddns_updates_successful_total[5m]) / rate(ddns_updates_total[5m])`
   - Display: Single stat, gauge

3. **API Errors**:
   - Query: `rate(ddns_api_errors_total[5m])`
   - Display: Time series graph

4. **Recent Errors**:
   - Query: Logs with `ERROR` level
   - Display: Log table

5. **Uptime**:
   - Query: `time() - process_start_time_seconds`
   - Display: Single stat

---

### Log Queries

**Find all DNS updates** (last hour):
```bash
sudo journalctl -u ddnsd --since "1 hour ago" | grep "DNS update"
```

**Find all errors** (last 24 hours):
```bash
sudo journalctl -u ddnsd --since "24 hours ago" | grep ERROR
```

**Find corruption events**:
```bash
sudo journalctl -u ddnsd | grep -i "corrupt"
```

**Find restart events**:
```bash
sudo journalctl -u ddnsd | grep "Starting ddnsd"
```

---

## Summary

**Logging Requirements**:
- ✅ Structured logging via `tracing` crate
- ✅ Log levels: TRACE, DEBUG, INFO, WARN, ERROR
- ✅ Key events logged (startup, updates, shutdown, errors)
- ❌ No secrets in logs (API tokens, etc.)
- ✅ Configurable log level (`DDNS_LOG_LEVEL`)

**Metrics Requirements** (Future):
- DNS update metrics (total, success, failure, duration)
- IP change metrics
- State store metrics
- API call metrics

**Health Check Requirements**:
- ✅ Process presence checks (all platforms)
- ✅ Configuration validation
- ✅ State store accessibility
- ⏳ HTTP health endpoint (future)

**Alerting Guidelines**:
- Critical: Daemon not running (15 min response)
- High: Repeated failures (10+ in 1 hour)
- Medium: State corruption (4 hours)
- Low: Single transient failure (info only)

**Monitoring Integration**:
- Logs: systemd journald, Docker logs, Kubernetes stdout/stderr
- Metrics: Future Prometheus endpoint
- Dashboards: Grafana panels for key metrics
- Alerts: Based on log patterns and future metrics

**Next Steps**:
- Phase 20 adds observability contract definition
- Future phases will implement Prometheus metrics endpoint
- HTTP health endpoint requires security consideration
- Log aggregation is platform-dependent

**For more information**:
- **Operations**: See `docs/OPS.md`
- **Crash Recovery**: See `docs/CRASH_RECOVERY.md`
- **Deployment**: See `docs/DEPLOYMENT.md`
