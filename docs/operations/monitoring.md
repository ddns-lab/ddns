# 📊 Monitoring Integration

Guide for monitoring ddnsd in production environments.

---

## ⚠️ Status: v0.1.1

**Currently implemented (v0.1.1)**:
- ✅ **Logging only** (via systemd journal)
- ✅ Structured log fields
- ✅ Log levels (trace/debug/info/warn/error)

**Not yet implemented (planned for future releases)**:
- ❌ Prometheus metrics endpoint
- ❌ Health check HTTP endpoint
- ❌ Native dashboard integration

---

## 📝 Log-Based Monitoring

**In v0.1.1**, all monitoring is done via log aggregation.

### Viewing Logs

**Real-time monitoring**:
```bash
# Follow logs
sudo journalctl -u ddnsd -f

# Filter by log level
sudo journalctl -u ddnsd -p info -f
```

**Export logs for aggregation**:
```bash
# Export to file
sudo journalctl -u ddnsd --since "1 hour ago" > /tmp/ddnsd-logs.json

# Export in JSON format
sudo journalctl -u ddnsd -o json > /tmp/ddnsd-logs.json
```

---

### Log Aggregation

#### ELK Stack (Elasticsearch, Logstash, Kibana)

**Forward journal logs to ELK**:

```bash
# Configure journal to forward to Logstash
# Edit /etc/systemd/journald.conf
sudo vi /etc/systemd/journald.conf

# Add:
[Logstash]
# Forward to Logstash
Server=logstash.example.com:5044
```

**Kibana query examples**:
```json
// Find all errors
{
  "query": {
    "match": {
      "UNIT": "ddnsd.service",
      "PRIORITY": "3"
    }
  }
}

// Find DNS update failures
{
  "query": {
    "match_phrase": {
      "MESSAGE": "API call failed"
    }
  }
}

// Find IP changes
{
  "query": {
    "match_phrase": {
      "MESSAGE": "IP change detected"
    }
  }
}
```

---

#### Grafana Loki

**Promtail configuration** (`/etc/promtail/config.yml`):

```yaml
server:
  http_listen_port: 9080

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://loki.example.com:3100/loki/api/v1/push

scrape_configs:
  - job_name: journal
    journal:
      max_age: 12h
      matches:
        - _SYSTEMD_UNIT=ddnsd.service
      labels:
        job: ddnsd
        unit: ddnsd
    relabel_configs:
      - source_labels: ['__journal__systemd_unit']
        target_label: 'unit'
      - source_labels: ['__journal_priority_keyword']
        target_label: 'level'
```

**LogQL queries**:

```logql
# All errors
{unit="ddnsd"} |= "ERROR"

# DNS update failures
{unit="ddnsd"} |= "API call failed"

# IP changes
{unit="ddnsd"} |= "IP change detected"

# Rate of DNS updates
rate({unit="ddnsd"} |= "DNS update successful"[5m])

# Error rate
rate({unit="ddnsd", level="err"}[5m])
```

---

## 🚨 Alerting Rules

### Alert on Errors

**Prometheus Alertmanager (via log metrics)**:

```yaml
# alerting_rules.yml
groups:
  - name: ddnsd
    interval: 30s
    rules:
      - alert: DdnsdHighErrorRate
        expr: |
          rate({unit="ddnsd", level="err"}[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High error rate detected"
          description: "{{ $value }} errors/sec for the last 5 minutes"

      - alert: DdnsdApiFailure
        expr: |
          count_over_time({unit="ddnsd"} |= "API call failed"[5m]) > 5
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Cloudflare API failures detected"
          description: "Multiple API call failures in logs"
```

---

### Alert on Service Down

**systemd monitoring**:

```bash
# Check if service is running
if ! systemctl is-active --quiet ddnsd; then
  # Send alert (mail, slack, etc.)
  echo "ddnsd service is down" | mail -s "Alert: ddnsd down" admin@example.com
fi
```

**Cron job** (`/etc/cron.d/ddnsd-monitor`):

```bash
# Check every 5 minutes
*/5 * * * * root systemctl is-active ddnsd || /usr/local/bin/alert-ddnsd-down.sh
```

---

## 📊 Key Metrics to Monitor

### Health Metrics

| Metric | How to Measure | Expected Value | Alert Threshold |
|--------|----------------|----------------|-----------------|
| Service status | `systemctl is-active ddnsd` | `active` | `inactive/failed` |
| Memory usage | `systemctl status ddnsd` (Memory) | ~13 MB | > 50 MB |
| CPU usage | `systemctl status ddnsd` (CPU) | ~0% | > 5% sustained |
| Uptime | `systemctl status ddnsd` | Days | < 1 minute (restart loop) |

### Log Metrics

| Metric | LogQL/journalctl Query | Expected Value | Alert Threshold |
|--------|----------------------|----------------|-----------------|
| Error rate | `rate({unit="ddnsd", level="err"}[5m])` | ~0 | > 0.01/sec |
| API failures | `count_over_time({unit="ddnsd"} |= "API call failed"[5m])` | 0 | > 3 in 5 min |
| IP changes | `count_over_time({unit="ddnsd"} |= "IP change detected"[1h])` | Variable | > 10/hour (unusual) |
| DNS updates | `count_over_time({unit="ddnsd"} |= "DNS update successful"[1h])` | Variable | < 1/day (no updates) |

---

## 🔍 Log Patterns to Monitor

### Warning Patterns

```bash
# Find retries (transient issues)
sudo journalctl -u ddnsd -p warn -n 100 | grep -i retry

# Find API rate limits
sudo journalctl -u ddnsd -p warn -n 100 | grep -i "rate limit"

# Find state store issues
sudo journalctl -u ddnsd -p warn -n 100 | grep -i "state"
```

### Error Patterns

```bash
# Find all errors
sudo journalctl -u ddnsd -p err -n 100

# Find API authentication failures
sudo journalctl -u ddnsd -p err -n 100 | grep -i "401\|403"

# Find DNS update failures
sudo journalctl -u ddnsd -p err -n 100 | grep -i "API call failed"

# Find network issues
sudo journalctl -u ddnsd -p err -n 100 | grep -i "timeout\|connection"
```

---

## 🎯 Dashboard Examples

### Grafana Dashboard (Log-based)

**Panel 1: Service Status**
```promql
# Query (via log metrics)
count({unit="ddnsd"})
```

**Panel 2: Error Rate**
```logql
rate({unit="ddnsd", level="err"}[5m])
```

**Panel 3: DNS Updates**
```logql
count_over_time({unit="ddnsd"} |= "DNS update successful"[1h])
```

**Panel 4: API Failures**
```logql
count_over_time({unit="ddnsd"} |= "API call failed"[1h])
```

---

## 📋 Monitoring Checklist

### Initial Setup

- [ ] Configure log aggregation (ELK/Loki)
- [ ] Set up alerting rules
- [ ] Create dashboard panels
- [ ] Test alert delivery (email/slack)

### Ongoing Monitoring

- [ ] Check service status daily
- [ ] Review error logs weekly
- [ ] Monitor resource usage weekly
- [ ] Test alert notifications monthly

---

## 🔄 Planned Features (Not Yet Implemented)

### Prometheus Metrics Endpoint

**Planned for future release**:

```yaml
# NOT YET IMPLEMENTED - Planned for v0.2.0 or later
# When implemented, will expose metrics at: http://localhost:9090/metrics

# Example metrics (planned):
ddnsd_up 1
ddnsd_ip_changes_total{record="example.com"} 5
ddnsd_dns_updates_total{record="example.com",status="success"} 5
ddnsd_api_errors_total{provider="cloudflare",error_type="timeout"} 0
ddnsd_memory_rss_bytes 13631488
```

**Integration with Prometheus**:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'ddnsd'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

---

### Health Check Endpoint

**Planned for future release**:

```yaml
# NOT YET IMPLEMENTED - Planned for v0.2.0 or later
# When implemented, will expose health at: http://localhost:9090/health

# Example response (planned):
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "ip_source_healthy": true,
  "provider_healthy": true,
  "state_store_healthy": true,
  "last_update": "2025-01-15T10:30:00Z"
}
```

---

## 🔗 Related Documentation

- [Observability Contract](observability.md) - Logging and metrics contract
- [Operations](ops.md) - Process lifecycle and operations
- [Crash Recovery](crash-recovery.md) - Incident response
- [Troubleshooting](../user/troubleshooting.md) - Common issues
