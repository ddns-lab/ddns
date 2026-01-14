# 🔧 Troubleshooting

Common issues and solutions for ddnsd.

---

## 🚨 Quick Diagnostics

**Before diving into specific issues**, run these diagnostic commands:

```bash
# 1. Check service status
sudo systemctl status ddnsd

# 2. Check recent errors
sudo journalctl -u ddnsd -p err -n 20

# 3. Check recent logs
sudo journalctl -u ddnsd -n 50

# 4. Verify configuration
sudo cat /etc/ddnsd/ddnsd.env

# 5. Test connectivity
ping -c 3 api.cloudflare.com
```

---

## 🔧 Common Issues

### Issue 1: Service Won't Start

**Symptom**:
```
sudo systemctl status ddnsd
# Shows: failed or inactive (dead)
```

**Possible causes**:
1. Configuration file missing or invalid
2. Binary not installed
3. Missing environment variables

**Solutions**:

**Check 1: Configuration file exists**
```bash
ls -l /etc/ddnsd/ddnsd.env
# Should show: -rw------- 1 root root ...
```

If missing:
```bash
sudo mkdir -p /etc/ddnsd
sudo vi /etc/ddnsd/ddnsd.env
# Add minimum configuration (see Configuration Guide)
```

**Check 2: Required variables are set**
```bash
sudo grep -E "DDNS_PROVIDER_API_TOKEN|DDNS_RECORDS|DDNS_STATE_STORE_PATH" /etc/ddnsd/ddnsd.env
```

If missing:
```bash
# Add required variables to /etc/ddnsd/ddnsd.env
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=example.com
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

**Check 3: Binary is installed**
```bash
which ddnsd
# Should show: /usr/local/bin/ddnsd

ddnsd --version
# Should show: ddnsd v0.1.1
```

If missing:
```bash
# Re-run installer
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

---

### Issue 2: DNS Records Not Updating

**Symptom**:
- Service is running
- Logs show "IP change detected"
- But DNS records not updating

**Possible causes**:
1. API token invalid or insufficient permissions
2. Record name format incorrect
3. Zone ID mismatch
4. Network connectivity issues

**Solutions**:

**Check 1: API token validity**
```bash
# Test API token
curl -X GET "https://api.cloudflare.com/client/v4/user/tokens/verify" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json"

# Expected response: {"success":true,"result":{"id":"..."}}
```

If token invalid:
- Regenerate token in Cloudflare Dashboard
- Ensure token has: Zone - DNS - Edit, Zone - Zone - Read

**Check 2: Record name format**
```bash
# Check logs for record format errors
sudo journalctl -u ddnsd -n 50 | grep -i record
```

**Correct formats**:
```bash
# Auto-detect (recommended)
DDNS_RECORDS=example.com

# Explicit IPv4
DDNS_RECORDS=example.com:A

# Explicit IPv6
DDNS_RECORDS=example.com:AAAA

# Multiple records
DDNS_RECORDS=example.com,www.example.com
```

**Check 3: Zone ID**
```bash
# If using DDNS_PROVIDER_ZONE_ID, verify it's correct
# Find zone ID in Cloudflare Dashboard → Your Zone → Overview → Zone ID
```

**Check 4: Network connectivity**
```bash
# Test Cloudflare API connectivity
curl -I https://api.cloudflare.com

# Expected: HTTP/1.1 200 OK or HTTP/2 200
```

---

### Issue 3: "400 Bad Request" from Cloudflare API

**Symptom**:
```
ERROR API call failed: 400 Bad Request
```

**Possible cause**: Record name format issue

**v0.1.1 fix**: This should be fixed in v0.1.1 (records are auto-created)

**Solution**:
```bash
# Ensure record format is correct
DDNS_RECORDS=example.com,www.example.com

# NOT: DDNS_RECORDS=example.com.,www.example.com.
# (No trailing dots)

# Restart service
sudo systemctl restart ddnsd
```

---

### Issue 4: IP Not Detected

**Symptom**:
```
WARN Failed to detect IP address
```

**Possible causes**:
1. Wrong interface specified
2. Interface has no IP address
3. Netlink socket creation failed (non-Linux)

**Solutions**:

**Check 1: Interface name**
```bash
# List network interfaces
ip link show

# Check which interface has IP
ip addr show

# Common names: eth0, ens18, wlan0, enp0s3
```

If using `DDNS_IP_SOURCE_INTERFACE`:
```bash
# Remove interface restriction (monitor all)
# Edit /etc/ddnsd/ddnsd.env
# Comment out: DDNS_IP_SOURCE_INTERFACE=eth0
# Or set to correct interface name

sudo systemctl restart ddnsd
```

**Check 2: Interface has IP address**
```bash
# Check for IPv4
ip -4 addr show eth0

# Check for IPv6
ip -6 addr show eth0
```

If no IP:
- Check network connectivity
- Check DHCP/NetworkManager

**Check 3: Netlink availability**
```bash
# Check if running on Linux
uname -s
# Should show: Linux

# If not Linux, use HTTP IP source
# Edit /etc/ddnsd/ddnsd.env
DDNS_IP_SOURCE_TYPE=http
DDNS_IP_SOURCE_URL=https://icanhazip.com
DDNS_IP_SOURCE_INTERVAL=300

sudo systemctl restart ddnsd
```

---

### Issue 5: "interface name has @ or if" in logs

**Symptom**:
```
WARN Detected interface: eth0@if14
```

**Cause**: veth interface (container/VPS)

**Solution**: This is normal for containerized environments. No action needed unless IP detection fails.

If IP detection fails:
```bash
# Specify parent interface instead
# Find parent interface
ip addr show | grep -E "^[0-9]+: "

# Edit /etc/ddnsd/ddnsd.env
DDNS_IP_SOURCE_INTERFACE=eth0  # Use parent interface (without @if)

sudo systemctl restart ddnsd
```

---

### Issue 6: Memory Usage Growing

**Symptom**:
```
# Memory usage grows over time
# Check with: sudo systemctl status ddnsd
```

**Possible cause**: State file corruption

**Solution**:
```bash
# Check state file size
ls -lh /var/lib/ddnsd/state.json

# If file is large (>1MB), backup and delete
sudo cp /var/lib/ddnsd/state.json /var/lib/ddnsd/state.json.backup
sudo rm /var/lib/ddnsd/state.json

# Restart service (will create new state file)
sudo systemctl restart ddnsd

# Monitor memory usage
watch -n 5 'sudo systemctl status ddnsd | grep Memory'
```

See [Crash Recovery Guide](../operations/crash-recovery.md) for state file recovery.

---

### Issue 7: How to View Logs

**Symptom**: "I don't know how to check what's happening"

**Solutions**:

**View recent logs**:
```bash
sudo journalctl -u ddnsd -n 50
```

**Follow logs in real-time**:
```bash
sudo journalctl -u ddnsd -f
```

**View errors only**:
```bash
sudo journalctl -u ddnsd -p err
```

**View logs from last boot**:
```bash
sudo journalctl -u ddnsd -b
```

**Export logs to file**:
```bash
sudo journalctl -u ddnsd > /tmp/ddnsd.log
```

---

### Issue 8: Configuration Changes Not Applied

**Symptom**: "I edited /etc/ddnsd/ddnsd.env but nothing changed"

**Cause**: Configuration is loaded once at startup

**Solution**:
```bash
# After editing configuration file:
sudo vi /etc/ddnsd/ddnsd.env

# MUST restart service for changes to take effect
sudo systemctl restart ddnsd

# Verify configuration loaded
sudo journalctl -u ddnsd -n 20 | grep -i "monitoring\|records"
```

**Important**: ddnsd does **not** support hot-reload. You must restart the service.

---

### Issue 9: API Token Validation Failing

**Symptom**:
```
ERROR API token validation failed
```

**Possible cause**: Token length or format issue

**v0.1.1 check**: Token must be 40 characters starting with 'd'

**Solution**:
```bash
# Check token length
echo -n "YOUR_TOKEN" | wc -c
# Should show: 40

# Check token format
echo "YOUR_TOKEN" | grep -E '^d[0-9a-f]{39}$'
# Should show: YOUR_TOKEN

# If token doesn't match:
# 1. Regenerate token in Cloudflare Dashboard
# 2. Ensure token length is exactly 40 characters
# 3. Update /etc/ddnsd/ddnsd.env
# 4. Restart service
```

---

### Issue 10: Service Restarting Continuously

**Symptom**:
```
sudo systemctl status ddnsd
# Shows: restarting continuously
```

**Possible causes**:
1. Crash on startup (configuration error)
2. Resource constraints
3. Binary corruption

**Solutions**:

**Check 1: Service logs**
```bash
sudo journalctl -u ddnsd -n 100 | tail -20
```

**Check 2: Configuration**
```bash
# Test configuration manually
sudo -u root ddnsd --once

# If this fails, configuration is invalid
```

**Check 3: Binary integrity**
```bash
# Reinstall binary
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

**Check 4: Disable auto-restart temporarily**
```bash
# Edit service file
sudo vi /etc/systemd/system/ddnsd.service

# Change: Restart=always
# To: Restart=no

sudo systemctl daemon-reload
sudo systemctl start ddnsd

# Check logs
sudo journalctl -u ddnsd -n 50

# Re-enable restart after fixing issue
```

---

## 📊 Diagnostic Commands Reference

| Check | Command | What it shows |
|-------|---------|---------------|
| Service status | `systemctl status ddnsd` | Running/failed, PID, memory |
| Recent logs | `journalctl -u ddnsd -n 50` | Last 50 log lines |
| Errors only | `journalctl -u ddnsd -p err` | Error-level logs |
| Follow logs | `journalctl -u ddnsd -f` | Real-time log stream |
| Configuration | `cat /etc/ddnsd/ddnsd.env` | Current configuration |
| State file | `ls -lh /var/lib/ddnsd/state.json` | State file size |
| API test | `curl -H "Authorization: Bearer TOKEN" https://api.cloudflare.com/client/v4/user/tokens/verify` | Token validity |
| IP addresses | `ip addr show` | All interface IPs |

---

## 🆘 Still Having Issues?

**Before asking for help**, gather this information:

```bash
# 1. Service status
sudo systemctl status ddnsd > /tmp/ddnsd-status.txt

# 2. Recent logs
sudo journalctl -u ddnsd -n 100 > /tmp/ddnsd-logs.txt

# 3. Configuration (remove sensitive data!)
sudo grep -v "API_TOKEN" /etc/ddnsd/ddnsd.env > /tmp/ddnsd-config.txt

# 4. Version
ddnsd --version > /tmp/ddnsd-version.txt

# 5. System info
uname -a > /tmp/ddnsd-system.txt
```

**Where to get help**:
- [GitHub Issues](https://github.com/ddns-lab/ddns/issues) - Bug reports
- [Documentation](../README.md) - Full documentation
- [Operations Guide](../operations/) - Production troubleshooting

---

## 🔗 Related Documentation

- [Installation](installation.md) - Installation issues
- [Configuration](configuration.md) - Configuration reference
- [Deployment](deployment.md) - Deployment issues
- [Operations](../operations/) - Production operations
- [Crash Recovery](../operations/crash-recovery.md) - State file corruption
