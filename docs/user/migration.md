# 🔄 Migration Guide

Upgrading ddnsd between versions.

---

## 📋 Version Upgrade Paths

**Current versions**:
- v0.1.1 → v0.1.2 (documentation release)
- v0.1.0 → v0.1.1 (previous feature release)

**Supported upgrade paths**:
- ✅ v0.1.0 → v0.1.2 (smooth upgrade, no breaking changes)
- ✅ v0.1.1 → v0.1.2 (smooth upgrade, documentation only)
- ✅ v0.1.2 → future versions (use standard upgrade procedure)

---

## 🚀 v0.1.1 → v0.1.2 Upgrade

### What's New in v0.1.2

**This is a documentation release**:
- 📚 Comprehensive documentation refactor
- 📚 New troubleshooting guide
- 📚 New migration guide
- 🧹 Removed duplicate and outdated documentation

### Breaking Changes

**None** - v0.1.2 is fully backward compatible with v0.1.1.

### Upgrade Procedure

Since this is a documentation-only release, **no code changes are required**.

**Optional actions**:
1. Review the new documentation structure
2. Bookmark the [Troubleshooting Guide](troubleshooting.md)
3. Check the [Operations Guide](../operations/) for production tips

---

## 🚀 v0.1.0 → v0.1.1 Upgrade

### What's New in v0.1.1

**New features**:
- ✅ Auto-create DNS records (records created automatically if missing)
- ✅ Better error messages
- ✅ Improved IP source validation
- ✅ Enhanced logging

**Bug fixes**:
- ✅ Fixed "400 Bad Request" error with record names
- ✅ Fixed interface name parsing (veth containers)
- ✅ Fixed API token validation (40-character check)

**Performance improvements**:
- ✅ Reduced memory overhead
- ✅ Faster startup time

---

### Breaking Changes

**None** - v0.1.1 is fully backward compatible with v0.1.0.

**Configuration changes**:
- No new required environment variables
- Existing configuration works as-is
- Optional new variables available (see below)

---

### New Environment Variables (Optional)

**v0.1.1 adds** these optional variables:

| Variable | Purpose | Default |
|----------|---------|---------|
| `DDNS_STARTUP_DELAY_SECS` | Delay before monitoring starts | `0` |
| `DDNS_MIN_UPDATE_INTERVAL_SECS` | Minimum time between DNS updates | `60` |

**Action required**: None (optional, use if needed)

**Example**:
```bash
# Add to /etc/ddnsd/ddnsd.env if desired
DDNS_STARTUP_DELAY_SECS=5  # Wait 5 seconds before starting
DDNS_MIN_UPDATE_INTERVAL_SECS=120  # Don't update more often than 2 minutes
```

---

### Upgrade Procedure

**Step 1: Backup Configuration**

```bash
# Backup configuration file
sudo cp /etc/ddnsd/ddnsd.env /etc/ddnsd/ddnsd.env.backup-$(date +%Y%m%d)

# Backup state file
sudo cp /var/lib/ddnsd/state.json /var/lib/ddnsd/state.json.backup-$(date +%Y%m%d)
```

---

**Step 2: Run Upgrade**

```bash
# Run installer (upgrade mode)
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

**The installer will**:
- Detect existing installation
- Prompt for confirmation (unless `DDNS_NONINTERACTIVE=true`)
- Replace the binary
- **Preserve your configuration** (`/etc/ddnsd/ddnsd.env`)
- **Preserve your state** (`/var/lib/ddnsd/state.json`)
- Restart the service automatically

---

**Step 3: Verify Upgrade**

```bash
# Check version
ddnsd --version
# Expected: ddnsd v0.1.1

# Check service status
sudo systemctl status ddnsd
# Expected: active (running)

# Check logs for successful startup
sudo journalctl -u ddnsd -n 20
# Expected: "INFO ddnsd v0.1.1 starting"
```

---

**Step 4: Test Functionality**

```bash
# Check logs for "Ready" message
sudo journalctl -u ddnsd -n 30 | grep "Ready"

# Verify DNS records are being monitored
sudo journalctl -u ddnsd -n 30 | grep "Monitoring"

# Trigger a manual update check (restart service)
sudo systemctl restart ddnsd

# Check for DNS updates
sudo journalctl -u ddnsd -n 50 | grep "DNS update"
```

---

### Post-Upgrade Verification Checklist

- [ ] Version shows `v0.1.1`
- [ ] Service is running (`systemctl status ddnsd`)
- [ ] Logs show no errors
- [ ] DNS records are updating correctly
- [ ] State file exists and is valid JSON
- [ ] Memory usage is normal (~13 MB)

---

### New Features in v0.1.1

#### Auto-Create DNS Records

**v0.1.0**: Records had to exist in Cloudflare manually
**v0.1.1**: Records are auto-created if missing

**What this means**:
- You can now add any domain to `DDNS_RECORDS`
- If the record doesn't exist, ddnsd will create it automatically
- Works for both A and AAAA records

**Example**:
```bash
# v0.1.1: This record will be auto-created
DDNS_RECORDS=new.example.com,www.example.com

# No need to manually create "new.example.com" in Cloudflare
```

#### Better Error Messages

**v0.1.0**: Generic error messages
**v0.1.1**: Specific, actionable error messages

**Examples**:
```
# v0.1.0
ERROR: API call failed

# v0.1.1
ERROR: API call failed: 401 Unauthorized
       Check your DDNS_PROVIDER_API_TOKEN
```

#### Enhanced Logging

**v0.1.1 adds**:
- More detailed IP change detection logs
- Better retry attempt logging
- Clearer startup messages

**Example log output**:
```
INFO ddnsd v0.1.1 starting
INFO Configuration loaded: 2 records, file state store
INFO IP source: netlink (all interfaces)
INFO DNS provider: cloudflare (zone: auto-detect)
INFO Monitoring 2 records: example.com (auto), www.example.com (auto)
INFO State loaded: last_ip=192.0.2.1, last_update=2025-01-15T10:30:00Z
INFO Ready, monitoring for IP changes...
```

---

## 🔄 Rollback Procedure

**If you need to rollback to v0.1.0**:

**Step 1: Stop service**
```bash
sudo systemctl stop ddnsd
```

**Step 2: Restore v0.1.0 binary**
```bash
# Download v0.1.0 binary
wget https://github.com/ddns-lab/ddns/releases/download/v0.1.0/ddnsd-linux-amd64.tar.gz

# Extract
tar -xzf ddnsd-linux-amd64.tar.gz

# Install
sudo install -m 755 ddnsd /usr/local/bin/ddnsd
```

**Step 3: Restore configuration** (if changed)
```bash
sudo cp /etc/ddnsd/ddnsd.env.backup-YYYYMMDD /etc/ddnsd/ddnsd.env
```

**Step 4: Start service**
```bash
sudo systemctl start ddnsd
```

**Step 5: Verify**
```bash
ddnsd --version
# Expected: ddnsd v0.1.0
```

---

## 📋 Upgrade Checklist

### Before Upgrade

- [ ] Read this guide completely
- [ ] Backup configuration file
- [ ] Backup state file
- [ ] Note current version (`ddnsd --version`)
- [ ] Check service is healthy (`systemctl status ddnsd`)

### During Upgrade

- [ ] Run installer script
- [ ] Confirm upgrade prompt (unless non-interactive)
- [ ] Wait for service restart

### After Upgrade

- [ ] Verify new version (`ddnsd --version`)
- [ ] Check service status (`systemctl status ddnsd`)
- [ ] Check logs for errors (`journalctl -u ddnsd -n 50`)
- [ ] Verify DNS updates work
- [ ] Monitor for 24 hours

---

## 🆘 Troubleshooting Upgrades

### Issue: Upgrade Script Fails

**Symptom**: Installer fails with error

**Solution**:
```bash
# Check logs
sudo journalctl -n 50

# Re-run with verbose output
bash -x install.sh 2>&1 | tee /tmp/install.log
```

---

### Issue: Service Won't Start After Upgrade

**Symptom**: Service fails to start after upgrade

**Solution**:
```bash
# Check logs
sudo journalctl -u ddnsd -n 100

# Test configuration manually
sudo -u root ddnsd --once

# If configuration is issue, restore backup
sudo cp /etc/ddnsd/ddnsd.env.backup-YYYYMMDD /etc/ddnsd/ddnsd.env
sudo systemctl restart ddnsd
```

---

### Issue: DNS Updates Not Working After Upgrade

**Symptom**: Service runs but DNS not updating

**Solution**:
```bash
# Check logs for API errors
sudo journalctl -u ddnsd -p err -n 20

# Verify API token still valid
curl -X GET "https://api.cloudflare.com/client/v4/user/tokens/verify" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json"

# Check if new features require configuration changes
# (v0.1.1 doesn't require changes)
```

---

## 🔗 Related Documentation

- [Changelog](../meta/changelog.md) - Complete version history
- [Installation](installation.md) - Installation guide
- [Configuration](configuration.md) - Configuration reference
- [Troubleshooting](troubleshooting.md) - Common issues
- [Versioning Policy](../meta/versioning.md) - Semantic versioning
