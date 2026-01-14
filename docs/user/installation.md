# 📥 Installation

This guide covers 3 ways to install ddnsd on Linux.

---

## ⚡ Quick Install (Recommended)

**One-line installation** - Fastest way to get ddnsd running:

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

This will:
- ✅ Download the latest ddnsd binary for your platform
- ✅ Install to `/usr/local/bin/ddnsd`
- ✅ Create systemd service with auto-start on boot
- ✅ Create configuration file at `/etc/ddnsd/ddnsd.env`

**What happens next:**
1. The installer will prompt for your Cloudflare API token and DNS records
2. Configuration is saved to `/etc/ddnsd/ddnsd.env`
3. The service is automatically enabled and started

---

## 📋 Platform Requirements

**Currently supported**:
- ✅ Linux (amd64)
- ✅ systemd (for service management)

**Not yet supported** (planned for future releases):
- ❌ Windows
- ❌ macOS
- ❌ ARM64 (temporarily disabled)
- ❌ Docker/Kubernetes (planned for v0.2.0)

---

## 🔧 Installation Methods

### Method 1: install.sh (Recommended)

**One-line installation**:

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

**Non-interactive mode** (for automation):

```bash
DDNS_NONINTERACTIVE=true curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

**Install specific version**:

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh -s - --version v0.1.1
```

**Advanced options**:

```bash
# Custom installation directory
DDNS_BINDIR=/opt/bin DDNS_CONFIGDIR=/etc/ddns-config curl -fsSL https://.../install.sh | sh

# Force systemd mode
curl -fsSL https://.../install.sh | sh -s - --mode systemd
```

**What gets installed**:
| Component | Location |
|-----------|----------|
| Binary | `/usr/local/bin/ddnsd` |
| Configuration | `/etc/ddnsd/ddnsd.env` |
| State file | `/var/lib/ddnsd/state.json` |
| Systemd service | `/etc/systemd/system/ddnsd.service` |

---

### Method 2: Pre-built Binary

**Download and install manually**:

```bash
# Download latest release
wget https://github.com/ddns-lab/ddns/releases/latest/download/ddnsd-linux-amd64.tar.gz

# Extract
tar -xzf ddnsd-linux-amd64.tar.gz

# Install
sudo install -m 755 ddnsd /usr/local/bin/ddnsd

# Verify
ddnsd --version
```

**Then create systemd service manually**:

```bash
# Create config directory
sudo mkdir -p /etc/ddnsd

# Create environment file
sudo vi /etc/ddnsd/ddnsd.env
# Add your configuration (see Configuration Guide)

# Create systemd service
sudo vi /etc/systemd/system/ddnsd.service
# Add service definition (see Deployment Guide)

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable ddnsd
sudo systemctl start ddnsd
```

---

### Method 3: Build from Source

**Build from source** (requires Rust toolchain):

```bash
# Clone repository
git clone https://github.com/ddns-lab/ddns.git
cd ddns

# Build release binary
cargo build --release

# Install
sudo install -m 755 target/release/ddnsd /usr/local/bin/ddnsd

# Verify
ddnsd --version
```

**Rust version**: Requires Rust 1.91 or later.

**Features**:
- Default: `cloudflare,netlink` (Cloudflare provider + Netlink IP source)
- All features: `cargo build --release --features all`

---

## ✅ Verification

After installation, verify that ddnsd is installed correctly:

```bash
# Check version
ddnsd --version

# Check systemd service status
sudo systemctl status ddnsd

# Check recent logs
sudo journalctl -u ddnsd -n 20
```

---

## 🔄 Upgrading

**Upgrade to latest version** using install.sh:

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

The installer will:
- ✅ Detect existing installation
- ✅ Prompt for upgrade confirmation
- ✅ Replace the binary
- ✅ **Preserve your configuration**
- ✅ Restart the service automatically

**Upgrade logs**:
```bash
# Check logs after upgrade
sudo journalctl -u ddnsd -n 20
```

---

## 🗑️ Uninstallation

**Remove ddnsd** from your system:

```bash
# Stop and disable service
sudo systemctl stop ddnsd
sudo systemctl disable ddnsd

# Remove binary
sudo rm /usr/local/bin/ddnsd

# Remove configuration (optional)
sudo rm -rf /etc/ddnsd

# Remove state file (optional)
sudo rm -rf /var/lib/ddnsd

# Remove systemd service
sudo rm /etc/systemd/system/ddnsd.service
sudo systemctl daemon-reload
```

---

## 🎯 Next Steps

After installation:

1. **Configure ddnsd**: See [Configuration Guide](configuration.md)
2. **Start the service**: See [Deployment Guide](deployment.md)
3. **Troubleshoot issues**: See [Troubleshooting Guide](troubleshooting.md)

---

## 🔗 Related Documentation

- [Configuration](configuration.md) - Complete environment variable reference
- [Deployment](deployment.md) - systemd deployment and verification
- [Troubleshooting](troubleshooting.md) - Common installation issues
- [Migration](migration.md) - Upgrading from v0.1.0
