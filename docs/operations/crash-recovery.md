# Crash Recovery & State Management Guide

This guide covers crash recovery semantics, state persistence, and disaster recovery procedures for the ddns daemon.

## Table of Contents

1. [Crash Recovery Semantics](#crash-recovery-semantics)
2. [State Persistence](#state-persistence)
3. [State File Corruption Recovery](#state-file-corruption-recovery)
4. [Disaster Recovery Procedures](#disaster-recovery-procedures)
5. [State File Management](#state-file-management)
6. [Troubleshooting](#troubleshooting)

---

## Crash Recovery Semantics

### Design Philosophy

The ddns daemon follows **crash-only software** principles:

- **No explicit shutdown required**: Process can be killed at any time
- **State always consistent**: State is atomically updated before DNS API calls
- **Idempotent operations**: Re-running after crash is safe
- **Automatic recovery**: Daemon recovers automatically on restart

### Crash Scenarios

| Scenario | Behavior | Data Loss | Recovery |
|----------|----------|-----------|----------|
| **SIGKILL** (kill -9) | Immediate termination | None (state persisted) | Automatic on restart |
| **Power failure** | Sudden power loss | Up to last update | Automatic on restart |
| **OOM killer** | Process terminated | Up to last update | Automatic on restart |
| **Crash during write** | Atomic writes prevent corruption | None (backup used) | Automatic from backup |
| **State file corruption** | Detected on load | Up to last backup | Automatic from backup |
| **Disk full** | Write fails | Current update lost | Automatic retry on next update |

### What's Persisted

**State file contains** (`/var/lib/ddns/state.json`):

```json
{
  "version": "1.0",
  "records": {
    "example.com": {
      "last_ip": "1.2.3.4",
      "last_updated": "2025-01-09T12:00:00Z",
      "provider_metadata": {}
    },
    "www.example.com": {
      "last_ip": "1.2.3.4",
      "last_updated": "2025-01-09T12:00:05Z",
      "provider_metadata": {}
    }
  }
}
```

**What's NOT persisted** (security):
- ❌ API tokens (environment only)
- ❌ Configuration (environment only)
- ❌ Temporary runtime state

### Crash Recovery Guarantees

**Guaranteed after crash**:
- ✅ Last successfully updated IP is known
- ✅ No duplicate DNS updates for same IP
- ✅ Daemon resumes monitoring immediately
- ✅ All records that succeeded before crash are preserved

**NOT guaranteed**:
- ❌ In-progress DNS update (will retry on restart)
- ❌ Updates that failed before crash (will retry on restart)
- ❌ Real-time IP changes during downtime (will update on next detection)

---

## State Persistence

### State Store Types

#### Memory State Store (`DDNS_STATE_STORE_TYPE=memory`)

**Characteristics**:
- No persistence across restarts
- Fastest performance (no I/O)
- Useful for testing or containerized deployments

**Crash behavior**:
- All state lost on crash/restart
- First run after crash treats all IPs as "new"
- Will attempt DNS update for all records on startup

**Use cases**:
- Testing environments
- Ephemeral containers (where restart is acceptable)
- Scenarios where initial DNS update is harmless

**Example**:
```bash
export DDNS_STATE_STORE_TYPE=memory
ddnsd
```

---

#### File State Store (`DDNS_STATE_STORE_TYPE=file`)

**Characteristics**:
- Persistent across restarts
- Atomic writes prevent corruption
- Automatic backup for recovery
- Slightly slower than memory (but negligible)

**Crash behavior**:
- State preserved across crash/restart
- Only records with changed IPs are updated
- Automatic recovery from corruption

**Use cases**:
- Production deployments
- Long-running daemons
- Scenarios where API quota conservation is important

**Example**:
```bash
export DDNS_STATE_STORE_TYPE=file
export DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
ddnsd
```

**File locations**:
- **Main file**: `/var/lib/ddns/state.json`
- **Backup file**: `/var/lib/ddns/state.backup` (automatic)
- **Temporary file**: `/var/lib/ddns/state.tmp` (during writes)

---

### Atomic Write Process

File state store uses atomic writes to prevent corruption:

```
1. Write to temporary file (state.tmp)
2. Flush to disk (fsync)
3. Copy current file to backup (state.backup)
4. Atomic rename: state.tmp → state.json
5. Remove temporary file
```

**Benefits**:
- **Crash-safe**: If crash occurs during write, old state file is intact
- **Always valid**: state.json is always complete JSON
- **Backup available**: If corruption detected, backup is used

**Why not direct writes?**
- Direct writes can result in partial JSON if crash occurs mid-write
- Atomic rename guarantees either old file or new file, never partial
- Backup provides fallback if new file is corrupted

---

### State File Format

**Version**: `1.0`

**Schema**:
```json
{
  "version": "string",        // Format version (for future migration)
  "records": {
    "record.name": {
      "last_ip": "string",    // Last known IP address
      "last_updated": "ISO8601",  // Timestamp of last update
      "provider_metadata": {}  // Provider-specific data (future use)
    }
  }
}
```

**Future compatibility**:
- Version field enables format migration
- Unknown fields in `provider_metadata` are ignored
- Missing fields use sensible defaults

---

## State File Corruption Recovery

### Corruption Detection

**Automatic detection on load**:

```bash
# Daemon starts
$ ddnsd
# State file corrupted: Failed to parse state file /var/lib/ddns/state.json: ...
# Attempting recovery from backup.
# Recovered state from backup: 2 records
# Restoring corrupted file from backup
```

**Detection triggers**:
- JSON parse errors
- Missing version field
- Version mismatch (warning only, still loads)
- Invalid IP address format
- Invalid timestamp format

### Automatic Recovery Process

**When corruption detected**:

1. **Detect**: JSON parse error on load
2. **Log warning**: "State file appears corrupted: ..."
3. **Load backup**: Try loading from `.backup` file
4. **Restore**: Copy backup to main file
5. **Continue**: Start daemon with recovered state
6. **Alert**: Log recovery for monitoring

**If backup also corrupted**:
1. **Log error**: "Backup also corrupted. Starting with empty state."
2. **Empty state**: Start with no previous IP knowledge
3. **Safe operation**: Will update all records on first IP change

**If backup missing**:
1. **Log warning**: "No backup file found. Starting with empty state."
2. **Empty state**: Start with no previous IP knowledge
3. **Safe operation**: Will update all records on first IP change

### Manual Recovery Procedures

#### If automatic recovery fails

**Procedure**:

```bash
# 1. Check if backup exists
ls -la /var/lib/ddns/

# Expected output:
# state.json        (corrupted)
# state.backup      (should exist)

# 2. Manually restore from backup
sudo cp /var/lib/ddns/state.backup /var/lib/ddns/state.json

# 3. Restart daemon
sudo systemctl restart ddnsd
```

#### If backup is also corrupted

**Procedure**:

```bash
# 1. Check for older backups (if you have rotation)
sudo ls -la /var/lib/ddns/backups/

# 2. Restore from oldest available backup
sudo cp /var/lib/ddns/backups/state.2025-01-08.json /var/lib/ddns/state.json

# 3. Restart daemon
sudo systemctl restart ddnsd

# 4. Verify operation
sudo journalctl -u ddnsd -n 20
```

#### If no backup exists

**Accept data loss and start fresh**:

```bash
# 1. Remove corrupted state file
sudo rm /var/lib/ddns/state.json

# 2. Restart daemon (will start with empty state)
sudo systemctl restart ddnsd

# 3. Verify operation (will update all records on next IP change)
sudo journalctl -u ddnsd -f
```

---

## Disaster Recovery Procedures

### Total Data Loss Scenario

**Scenario**: Disk failure, state file completely lost

**Impact**:
- No record of last IP addresses
- Daemon will update all records on next IP change
- No service disruption (just unnecessary API calls)

**Recovery**:

```bash
# 1. Create new state directory
sudo mkdir -p /var/lib/ddns
sudo chown ddns:ddns /var/lib/ddns
sudo chmod 750 /var/lib/ddns

# 2. Start daemon (will create new state file)
sudo systemctl start ddnsd

# 3. Verify state file created
ls -la /var/lib/ddns/state.json

# 4. Monitor first update
sudo journalctl -u ddnsd -f
# Should see: "DNS update successful" for each record
```

**Prevention**:

```bash
# Regular backups (add to cron)
0 0 * * * cp /var/lib/ddns/state.json /var/lib/ddns/backups/state.$(date +\%Y-\%m-\%d).json

# Or use systemd timer
# /etc/systemd/system/ddns-backup.service
[Unit]
Description=Backup ddns state file

[Service]
Type=oneshot
ExecStart=/bin/cp /var/lib/ddns/state.json /var/lib/ddns/backups/state.$(date +\%%Y-\\%m-\\%d).json

# /etc/systemd/system/ddns-backup.timer
[Unit]
Description=Daily ddns state backup

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

### State File Migration

**Scenario**: Migrating to new server, restoring state from old server

**Procedure**:

```bash
# 1. Copy state file from old server
scp user@old-server:/var/lib/ddns/state.json /tmp/

# 2. Copy to new server
sudo cp /tmp/state.json /var/lib/ddns/state.json

# 3. Fix permissions
sudo chown ddns:ddns /var/lib/ddns/state.json
sudo chmod 640 /var/lib/ddns/state.json

# 4. Verify JSON is valid
jq '.' /var/lib/ddns/state.json

# 5. Start daemon
sudo systemctl start ddnsd

# 6. Verify loaded state
sudo journalctl -u ddnsd -n 10
# Should show: "Loaded state from file: N records"
```

### Multiple Instances Conflict

**Scenario**: Two instances accidentally running, writing to same state file

**Symptoms**:
- State file corruption
- Conflicting logs
- Race conditions

**Recovery**:

```bash
# 1. Stop all instances
sudo systemctl stop ddnsd
sudo pkill ddnsd  # Kill any remaining

# 2. Verify only one instance
ps aux | grep ddnsd

# 3. Restore state from backup
sudo cp /var/lib/ddns/state.backup /var/lib/ddns/state.json

# 4. Start single instance
sudo systemctl start ddnsd

# 5. Verify
sudo systemctl status ddnsd
```

**Prevention**:

```bash
# systemd already prevents multiple instances
# (Type=simple, only one service instance)

# For manual runs, use pidfile or check lock
```

---

## State File Management

### File Permissions

**Recommended permissions**:

```bash
# Directory
drwxr-x---  ddns ddns /var/lib/ddns/

# State file
-rw-r-----  ddns ddns /var/lib/ddns/state.json

# Backup file
-rw-r-----  ddns ddns /var/lib/ddns/state.backup
```

**Why**: Limits read access to ddns user and root only. Prevents other users from seeing DNS records.

**Set permissions**:

```bash
sudo chown -R ddns:ddns /var/lib/ddns
sudo chmod 750 /var/lib/ddns
sudo chmod 640 /var/lib/ddns/state.json
sudo chmod 640 /var/lib/ddns/state.backup
```

### File Rotation

**Automatic backup**: Every write creates a `.backup` file

**Manual rotation** (optional):

```bash
# 1. Create backups directory
sudo mkdir -p /var/lib/ddns/backups

# 2. Rotate backup
sudo cp /var/lib/ddns/state.backup /var/lib/ddns/backups/state.$(date +%Y%m%d-%H%M%S).json

# 3. Keep last 7 days
find /var/lib/ddns/backups -name "state.*" -mtime +7 -delete
```

### State File Validation

**Manual validation**:

```bash
# Check JSON is valid
jq '.' /var/lib/ddns/state.json

# Check version
jq '.version' /var/lib/ddns/state.json

# Count records
jq '.records | length' /var/lib/ddns/state.json

# List all records
jq '.records | keys[]' /var/lib/ddns/state.json

# Check specific record
jq '.records["example.com"]' /var/lib/ddns/state.json
```

**Automated validation**:

```bash
#!/bin/bash
# validate-state.sh

STATE_FILE="/var/lib/ddns/state.json"

# Check file exists
if [ ! -f "$STATE_FILE" ]; then
    echo "ERROR: State file missing"
    exit 1
fi

# Check permissions
PERMS=$(stat -c %a "$STATE_FILE")
if [ "$PERMS" != "640" ]; then
    echo "WARNING: Wrong permissions: $PERMS (expected 640)"
fi

# Validate JSON
if ! jq -e '.' "$STATE_FILE" > /dev/null 2>&1; then
    echo "ERROR: State file is not valid JSON"
    exit 1
fi

# Check version
VERSION=$(jq -r '.version' "$STATE_FILE")
if [ "$VERSION" != "1.0" ]; then
    echo "WARNING: Unknown version: $VERSION (expected 1.0)"
fi

# Check records
RECORD_COUNT=$(jq '.records | length' "$STATE_FILE")
echo "State file valid: $RECORD_COUNT record(s)"
```

### State File Size

**Typical size**:

- **1 record**: ~200 bytes
- **10 records**: ~2 KB
- **100 records**: ~20 KB

**Monitor size**:

```bash
# Check current size
du -h /var/lib/ddns/state.json

# Alert if too large (>1 MB)
if [ $(stat -f%z /var/lib/ddns/state.json) -gt 1048576 ]; then
    echo "WARNING: State file too large"
fi
```

**Why it grows**:
- Each record adds ~200 bytes
- Provider metadata can add more
- Should never exceed 1 MB in normal operation

---

## Troubleshooting

### State File Not Loading

**Symptom**: Daemon starts but logs "Starting with empty state"

**Diagnosis**:

```bash
# Check if state file exists
ls -la /var/lib/ddns/state.json

# Check permissions
namei -l /var/lib/ddns/state.json

# Check file content
head -20 /var/lib/ddns/state.json
```

**Common causes**:
1. **File doesn't exist**: Normal for first run
2. **Wrong permissions**: Fix with `sudo chmod 640 /var/lib/ddns/state.json`
3. **Wrong path**: Check `DDNS_STATE_STORE_PATH` environment variable
4. **Corrupted file**: Automatic recovery from backup

### State File Permission Errors

**Symptom**: "Permission denied" errors

**Fix**:

```bash
# Fix ownership
sudo chown -R ddns:ddns /var/lib/ddns

# Fix permissions
sudo chmod 750 /var/lib/ddns
sudo chmod 640 /var/lib/ddns/state.json
```

### State File Corruption

**Symptom**: "Failed to parse state file" in logs

**Diagnosis**:

```bash
# Check JSON validity
jq '.' /var/lib/ddns/state.json

# Check backup
ls -la /var/lib/ddns/state.backup
jq '.' /var/lib/ddns/state.backup
```

**Recovery**:

```bash
# If backup is valid, restore it
sudo cp /var/lib/ddns/state.backup /var/lib/ddns/state.json
sudo systemctl restart ddnsd

# If backup is also corrupted, accept data loss
sudo rm /var/lib/ddns/state.json
sudo systemctl restart ddnsd
```

### State File Growing Too Large

**Symptom**: State file >1 MB

**Diagnosis**:

```bash
# Count records
jq '.records | length' /var/lib/ddns/state.json

# Find large records
jq '.records | to_entries | .[] | select(.value | length > 1000)' /var/lib/ddns/state.json
```

**Fix**:

```bash
# If too many records, remove unused ones
# Edit state file (carefully!)
sudo cp /var/lib/ddns/state.json /var/lib/ddns/state.json.bak
sudo nano /var/lib/ddns/state.json
# Remove unused record entries
sudo systemctl restart ddnsd

# Or just delete and start fresh
sudo rm /var/lib/ddns/state.json
sudo systemctl restart ddnsd
```

### State File Location Changes

**Scenario**: Need to move state file to different location

**Procedure**:

```bash
# 1. Stop daemon
sudo systemctl stop ddnsd

# 2. Move state file
sudo mv /var/lib/ddns/state.json /new/location/state.json

# 3. Update environment file
sudo nano /etc/default/ddnsd
# Change: DDNS_STATE_STORE_PATH=/new/location/state.json

# 4. Create new directory if needed
sudo mkdir -p /new/location
sudo chown ddns:ddns /new/location
sudo chmod 750 /new/location

# 5. Update systemd unit if path is hardcoded (shouldn't be)
# Usually not needed if using environment variables

# 6. Start daemon
sudo systemctl start ddnsd
```

---

## Summary

**Crash Recovery Guarantees**:
- ✅ State atomically persisted before DNS updates
- ✅ Automatic recovery from corruption using backup
- ✅ No data loss if backup exists
- ✅ Safe to kill/crash at any time

**State Persistence**:
- **Memory store**: Fast, no persistence, use for testing
- **File store**: Persistent, atomic writes, automatic backup

**Corruption Recovery**:
- **Automatic**: Detected on load, backup restored
- **Manual**: Restore from backup if auto-recovery fails
- **Last resort**: Accept data loss, start with empty state

**Best Practices**:
- Use file store in production
- Regular backups of state file
- Monitor for corruption errors
- Validate state file JSON periodically
- Keep backup directory with retention policy

**For more information**:
- **Operations**: See `docs/OPS.md`
- **Deployment**: See `docs/DEPLOYMENT.md`
- **Security**: See `docs/SECURITY.md`
