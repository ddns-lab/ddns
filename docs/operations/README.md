# 🔧 Operations Guide

Essential guides for running ddnsd in production environments.

---

## 🎯 Production Operations

This section covers monitoring, incident response, and maintenance for ddnsd deployments.

### 📊 Monitoring & Observability

- **[Observability](observability.md)** - Logging, metrics, health checks
  - Log levels and formats
  - Structured logging fields
  - Health check endpoints (planned)

- **[Monitoring Integration](monitoring.md)** - Prometheus/Grafana setup (planned)
  - Log aggregation (ELK/Loki)
  - Alert rules
  - Dashboard examples

### 🚨 Incident Response

- **[Crash Recovery](crash-recovery.md)** - State corruption and recovery procedures
  - Crash recovery semantics
  - State file corruption recovery
  - Disaster recovery procedures
  - Backup and restore

- **[Operations](ops.md)** - Signal handling and process lifecycle
  - Process lifecycle (startup/shutdown)
  - Signal handling (SIGTERM, SIGHUP, SIGUSR1)
  - Error classification and handling
  - systemd service management

### 🔧 Maintenance

- **[Secret Rotation](secret-rotation.md)** - API token rotation procedures
  - Rotation strategies
  - Platform-specific steps (systemd/Docker)
  - Verification and rollback

- **[Migration](../user/migration.md)** - Version upgrade procedures
  - Version upgrade guides (see Migration Guide)
  - Breaking changes assessment
  - Rollback procedures

---

## 🎯 Operator Checklist

### Pre-Deployment

- [ ] Review [Security Guide](../security/security.md)
- [ ] Configure state store backup
- [ ] Set up log aggregation
- [ ] Prepare monitoring dashboard

### Post-Deployment

- [ ] Verify service is running: `systemctl status ddnsd`
- [ ] Check logs: `journalctl -u ddnsd -n 50`
- [ ] Confirm DNS updates work
- [ ] Test crash recovery

### Ongoing Operations

- [ ] Monitor logs for errors
- [ ] Review state file integrity
- [ ] Plan secret rotation schedule
- [ ] Test upgrade procedures in staging

---

## 📋 Key Metrics

Monitor these metrics to ensure healthy operation:

### Resource Usage

| Metric | Expected | Alert Threshold |
|--------|----------|-----------------|
| Memory (RSS) | ~13 MB | > 50 MB |
| CPU (idle) | ~0% | > 5% sustained |
| Startup time | ~20 ms | > 1 second |

### Health Indicators

| Check | Command | Healthy Result |
|-------|---------|----------------|
| Service status | `systemctl is-active ddnsd` | `active` |
| Recent errors | `journalctl -u ddnsd -p err -n 10` | No output |
| State file | `stat /var/lib/ddnsd/state.json` | File exists |

### Log Patterns to Monitor

- **ERROR level**: Any ERROR level logs should be investigated
- **API failures**: Repeated "API call failed" messages
- **State store errors**: "Failed to read/write state"
- **IP source errors**: "Failed to detect IP address"

---

## 🔍 Troubleshooting

**Common operational issues**:

| Symptom | First Check | Reference |
|---------|-------------|-----------|
| Service not running | `systemctl status ddnsd` | [Ops](ops.md) |
| High memory usage | Check state file size | [Crash Recovery](crash-recovery.md) |
| DNS not updating | API token validity | [Secret Rotation](secret-rotation.md) |
| Log errors | `journalctl -u ddnsd -p err` | [Observability](observability.md) |

For more issues, see [User Troubleshooting Guide](../user/troubleshooting.md).

---

## 📞 Escalation

### Self-Service

- **Common issues**: [User Troubleshooting](../user/troubleshooting.md)
- **Documentation**: [docs/](../README.md)

### Community Support

- **GitHub Issues**: [Report a bug](https://github.com/ddns-lab/ddns/issues)
- **Architecture**: [Architecture Documentation](../architecture/)

### Security Issues

- **Security Policy**: [SECURITY.md](../../SECURITY.md)
- **Security Guide**: [Security Documentation](../security/security.md)

---

## ⏱️ Time Investment

| Task | Time |
|------|------|
| Initial deployment | 15 minutes |
| Monitoring setup | 30 minutes |
| Incident response practice | 1 hour |
| Secret rotation procedure | 15 minutes |

---

## 🔗 Related Documentation

- [User Guide](../user/) - Installation and configuration
- [Architecture](../architecture/) - System design and internals
- [Security](../security/security.md) - Security best practices
