# 🔄 Migration Guide

Upgrading ddnsd between versions.

---

## 📋 Version Upgrade Paths

**Current versions**:
- v0.2.0 → v0.2.1 (breaking change: environment variables)
- v0.1.2 → v0.2.0 (major refactoring)

**Supported upgrade paths**:
- ✅ v0.2.0 → v0.2.1 (breaking change: config update required)
- ⚠️ v0.1.x → v0.2.1 (breaking change: config update required)

---

## 🚨 v0.2.0 → v0.2.1 Upgrade

### ⚠️ Breaking Change: Environment Variables

**v0.2.1 removes backwards compatibility** with old environment variable names. All providers now require the `DDNS_` prefix.

### Required Configuration Updates

You **must** update your `/etc/ddnsd/ddnsd.env` to use new variable names:

**Cloudflare**:
```bash
# OLD (no longer works in v0.2.1)
DDNS_PROVIDER_API_TOKEN=xxx
DDNS_PROVIDER_ZONE_ID=xxx

# NEW (required)
DDNS_CLOUDFLARE_API_TOKEN=xxx
DDNS_CLOUDFLARE_ZONE_ID=xxx
```

**Aliyun**:
```bash
# OLD (no longer works in v0.2.1)
DDNS_PROVIDER_API_TOKEN=xxx

# NEW (required)
DDNS_ALIYUN_ACCESS_KEY_ID=xxx
DDNS_ALIYUN_ACCESS_KEY_SECRET=xxx
```

**NameSilo**:
```bash
# OLD (no longer works in v0.2.1)
DDNS_PROVIDER_API_KEY=xxx

# NEW (required)
DDNS_NAMESILO_API_KEY=xxx
```

**GoDaddy**:
```bash
# OLD (no longer works in v0.2.1)
DDNS_PROVIDER_API_KEY=xxx
DDNS_PROVIDER_API_SECRET=xxx

# NEW (required)
DDNS_GODADDY_API_KEY=xxx
DDNS_GODADDY_API_SECRET=xxx
DDNS_GODADDY_OTE=true  # optional
```

---

### Upgrade Procedure

**Step 1: Uninstall v0.2.0**

```bash
# Complete uninstall (preserves config)
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/uninstall.sh | sh
```

---

**Step 2: Backup and Update Configuration**

```bash
# Backup old config
sudo cp /etc/ddnsd/ddnsd.env /etc/ddnsd/ddnsd.env.v0.2.0.backup

# Update config to use new variable names
sudo vi /etc/ddnsd/ddnsd.env
```

**Replace old variable names with new ones** (see examples above).

---

**Step 3: Reinstall v0.2.1**

```bash
# Install v0.2.1
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh

# The installer will preserve your updated config
```

---

**Step 4: Verify v0.2.1**

```bash
# Check version
ddnsd --version
# Expected: ddnsd 0.2.1

# Start service
sudo systemctl start ddnsd

# Check service status
sudo systemctl status ddnsd

# Check logs for errors
sudo journalctl -u ddnsd -n 50
```

---

### What's New in v0.2.1

- ✅ Clean environment variable naming (all DDNS_ prefix)
- ✅ Provider-specific credentials (no generic DDNS_PROVIDER_API_TOKEN)
- ✅ Added `uninstall.sh` script for complete removal
- ✅ Auto-detection of non-interactive mode in pipe installations
- ✅ Tested on Linux with all providers

---

### Migration Checklist

- [ ] Backup old configuration file
- [ ] Update all environment variable names to use DDNS_ prefix
- [ ] Verify provider-specific variables are correct
- [ ] Uninstall v0.2.0
- [ ] Install v0.2.1
- [ ] Verify version: `ddnsd --version` shows `0.2.1`
- [ ] Service starts without errors
- [ ] Check logs: no "environment variable not found" errors
- [ ] DNS updates working correctly

---

## 🚀 v0.1.2 → v0.2.1 Upgrade

### ⚠️ Breaking Change: Environment Variables

Upgrading from v0.1.x to v0.2.1 requires configuration updates. See the v0.2.0 → v0.2.1 section above for detailed migration instructions.

**Summary of changes**:
- Old generic variables (`DDNS_PROVIDER_API_TOKEN`) no longer supported
- All providers use DDNS_ prefix
- Provider-specific variables are now required

**Quick migration from v0.1.x**:

```bash
# 1. Uninstall v0.1.x
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/uninstall.sh | sh -s -- --purge-all

# 2. Install v0.2.1 (generates new config template)
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh

# 3. Edit config with new variable names
sudo vi /etc/ddnsd/ddnsd.env

# 4. Start service
sudo systemctl start ddnsd
```

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
- Prompt for confirmation (unless running in non-interactive mode)
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
       Check your DDNS_CLOUDFLARE_API_TOKEN
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

**If you need to rollback to a previous version**:

**Step 1: Stop service**
```bash
sudo systemctl stop ddnsd
```

**Step 2: Restore previous binary**
```bash
# Download specific version binary
wget https://github.com/ddns-lab/ddns/releases/download/v0.2.0/ddnsd-linux-amd64.tar.gz

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
# Expected: ddnsd v0.2.0 (or whatever version you rolled back to)
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
sudo ddnsd --config-test

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
# For Cloudflare:
curl -X GET "https://api.cloudflare.com/client/v4/user/tokens/verify" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json"

# Check if new features require configuration changes
# See migration guide for your version
```

---

### Issue: Environment Variable Not Found

**Symptom** (v0.2.0 → v0.2.1 upgrade):
```
Configuration error: DDNS_CLOUDFLARE_API_TOKEN is required
```

**Solution**:
```bash
# You need to update your config file
sudo vi /etc/ddnsd/ddnsd.env

# Change old variable names to new ones:
# DDNS_PROVIDER_API_TOKEN → DDNS_CLOUDFLARE_API_TOKEN
# DDNS_PROVIDER_ZONE_ID → DDNS_CLOUDFLARE_ZONE_ID

# Restart service
sudo systemctl restart ddnsd
```

---

## 🔗 Related Documentation

- [Changelog](../meta/changelog.md) - Complete version history
- [Installation](installation.md) - Installation guide
- [Configuration](configuration.md) - Configuration reference
- [Troubleshooting](troubleshooting.md) - Common issues
- [Versioning Policy](../meta/versioning.md) - Semantic versioning
