# 🚀 Deployment

Deploy ddnsd with systemd on Linux.

---

## ⚡ Quick Start

**If you used install.sh**, ddnsd is already deployed. Skip to [Verification](#verification).

```bash
# One-line installation (includes deployment)
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

---

## 📋 Prerequisites

**Before deploying**:
- ✅ ddnsd binary installed (see [Installation Guide](installation.md))
- ✅ Configuration file created at `/etc/ddnsd/ddnsd.env`
- ✅ Linux with systemd
- ✅ Cloudflare API token
- ✅ Root or sudo access

**Not yet supported**:
- ❌ Docker deployment (planned for v0.2.0)
- ❌ Kubernetes deployment (planned for v0.2.0)

---

## 🔧 Systemd Deployment

### Step 1: Create Configuration

**Ensure configuration file exists**:

```bash
# Create config directory
sudo mkdir -p /etc/ddnsd

# Create configuration file
sudo vi /etc/ddnsd/ddnsd.env
```

**Minimum configuration**:

```bash
# /etc/ddnsd/ddnsd.env
DDNS_PROVIDER_API_TOKEN=your_cloudflare_token
DDNS_RECORDS=example.com,www.example.com
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json
```

See [Configuration Guide](configuration.md) for all options.

---

### Step 2: Create Systemd Service File

**Create service unit**:

```bash
sudo vi /etc/systemd/system/ddnsd.service
```

**Service file content**:

```ini
[Unit]
Description=Dynamic DNS Daemon
Documentation=https://github.com/ddns-lab/ddns
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
EnvironmentFile=/etc/ddnsd/ddnsd.env
ExecStart=/usr/local/bin/ddnsd
Restart=always
RestartSec=5s
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/etc/ddnsd /var/lib/ddnsd

[Install]
WantedBy=multi-user.target
```

**What this does**:
- Starts after network is ready
- Auto-restarts on failure (after 5 seconds)
- Logs to systemd journal
- Security hardening (restricted permissions)

---

### Step 3: Create State Directory

**Create state file directory**:

```bash
# Create directory
sudo mkdir -p /var/lib/ddnsd

# Set permissions
sudo chmod 755 /var/lib/ddnsd
sudo chown root:root /var/lib/ddnsd
```

---

### Step 4: Enable and Start Service

**Reload systemd and enable service**:

```bash
# Reload systemd configuration
sudo systemctl daemon-reload

# Enable auto-start on boot
sudo systemctl enable ddnsd

# Start service
sudo systemctl start ddnsd
```

---

## ✅ Verification

### Check Service Status

```bash
# Check if service is running
sudo systemctl status ddnsd
```

**Expected output**:
```
● ddnsd.service - Dynamic DNS Daemon
     Loaded: loaded (/etc/systemd/system/ddnsd.service; enabled)
     Active: active (running) since Mon 2025-01-15 10:30:00 UTC
   Main PID: 1234 (ddnsd)
      Tasks: 5 (limit: 1900)
     Memory: 13.2M
        CPU: 15ms
     CGroup: /system.slice/ddnsd.service
             └─1234 /usr/local/bin/ddnsd
```

**Key indicators**:
- ✅ `Active: active (running)`
- ✅ Memory usage ~13 MB
- ✅ CPU minimal

---

### Check Logs

```bash
# View recent logs
sudo journalctl -u ddnsd -n 50

# Follow logs in real-time
sudo journalctl -u ddnsd -f

# View errors only
sudo journalctl -u ddnsd -p err
```

**Expected log output**:

```
Jan 15 10:30:00 server ddnsd[1234]: INFO ddnsd v0.1.1 starting
Jan 15 10:30:00 server ddnsd[1234]: INFO IP source: netlink
Jan 15 10:30:00 server ddnsd[1234]: INFO DNS provider: cloudflare
Jan 15 10:30:00 server ddnsd[1234]: INFO Monitoring 2 records: example.com, www.example.com
Jan 15 10:30:00 server ddnsd[1234]: INFO State store: file at /var/lib/ddnsd/state.json
Jan 15 10:30:00 server ddnsd[1234]: INFO Ready, monitoring for IP changes...
```

---

### Test DNS Update

**Trigger an update** (if IP hasn't changed recently):

```bash
# Restart service to force update check
sudo systemctl restart ddnsd

# Check logs for update
sudo journalctl -u ddnsd -n 20
```

**Expected log output**:

```
Jan 15 10:35:00 server ddnsd[1234]: INFO IP change detected: 192.0.2.1
Jan 15 10:35:00 server ddnsd[1234]: INFO Updating example.com → 192.0.2.1
Jan 15 10:35:00 server ddnsd[1234]: INFO DNS update successful
```

---

### Verify DNS Record

**Check that DNS record was updated**:

```bash
# Query DNS record
dig example.com +short

# Expected: Your current IP address
192.0.2.1
```

**Or check Cloudflare Dashboard**:
1. Go to DNS → Records
2. Verify `example.com` points to your IP
3. Check "Last modified" timestamp

---

## 🔄 Service Management

### Common Commands

```bash
# Start service
sudo systemctl start ddnsd

# Stop service
sudo systemctl stop ddnsd

# Restart service
sudo systemctl restart ddnsd

# Check status
sudo systemctl status ddnsd

# Enable auto-start on boot
sudo systemctl enable ddnsd

# Disable auto-start
sudo systemctl disable ddnsd

# Reload configuration (after editing .env file)
sudo systemctl daemon-reload
sudo systemctl restart ddnsd
```

---

## 📊 Monitoring

### Resource Usage

```bash
# Check resource usage
sudo systemctl status ddnsd

# Or use top/htop
top -p $(pgrep ddnsd)
```

**Expected usage** (idle):
- Memory: ~13 MB RSS
- CPU: ~0%
- Startup: ~20 ms

---

### Log Monitoring

**Continuous log monitoring**:

```bash
# Follow logs
sudo journalctl -u ddnsd -f

# Filter by log level
sudo journalctl -u ddnsd -p info -f

# Show last 100 lines
sudo journalctl -u ddnsd -n 100
```

**Key log patterns**:
- `INFO IP change detected` - IP address changed
- `INFO DNS update successful` - DNS updated successfully
- `ERROR API call failed` - Cloudflare API error (check token)
- `WARN Retrying` - Retry in progress

---

## 🔧 Troubleshooting Deployment Issues

### Service Won't Start

**Symptom**: `systemctl status ddnsd` shows `failed`

**Possible causes**:
1. Configuration file missing or invalid
2. Binary not installed
3. Missing permissions

**Solutions**:
```bash
# Check configuration file exists
ls -l /etc/ddnsd/ddnsd.env

# Check binary exists
ls -l /usr/local/bin/ddnsd

# Check service logs
sudo journalctl -u ddnsd -n 50 --no-pager

# Test configuration manually
source /etc/ddnsd/ddnsd.env
ddnsd --help
```

---

### Permission Errors

**Symptom**: Logs show "Permission denied"

**Solution**:
```bash
# Fix state directory permissions
sudo chmod 755 /var/lib/ddnsd
sudo chown root:root /var/lib/ddnsd

# Fix config file permissions
sudo chmod 600 /etc/ddnsd/ddnsd.env
sudo chown root:root /etc/ddnsd/ddnsd.env
```

---

### DNS Records Not Updating

**Symptom**: Service runs but DNS doesn't update

**Check**:
1. API token is valid
2. Records are in correct format
3. Network connectivity

```bash
# Check logs for API errors
sudo journalctl -u ddnsd -p err -n 20

# Test API token
curl -X GET "https://api.cloudflare.com/client/v4/user/tokens/verify" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type:application/json"
```

---

## 🔄 Upgrading

**Upgrade ddnsd**:

```bash
# Run installer (upgrade mode)
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh

# Installer will:
# - Detect existing installation
# - Replace binary
# - Preserve configuration
# - Restart service automatically
```

**Verify upgrade**:

```bash
# Check version
ddnsd --version

# Check service status
sudo systemctl status ddnsd

# Check logs
sudo journalctl -u ddnsd -n 20
```

See [Migration Guide](migration.md) for version-specific upgrade notes.

---

## 🗑️ Uninstallation

**Remove ddnsd completely**:

```bash
# Stop and disable service
sudo systemctl stop ddnsd
sudo systemctl disable ddnsd

# Remove service file
sudo rm /etc/systemd/system/ddnsd.service
sudo systemctl daemon-reload

# Remove binary
sudo rm /usr/local/bin/ddnsd

# Remove configuration and state (optional)
sudo rm -rf /etc/ddnsd
sudo rm -rf /var/lib/ddnsd
```

---

## 🎯 Next Steps

- **Configuration**: See [Configuration Guide](configuration.md)
- **Monitoring**: See [Operations Guide](../operations/)
- **Troubleshooting**: See [Troubleshooting Guide](troubleshooting.md)

---

## 🔗 Related Documentation

- [Installation](installation.md) - Installing ddnsd
- [Configuration](configuration.md) - Environment variable reference
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [Operations](../operations/) - Production operations guide
