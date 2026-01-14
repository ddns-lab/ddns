# 👤 User Guide

Complete guide for installing, configuring, and running ddnsd.

---

## 🎯 New to ddns?

Follow this path to get ddnsd running in ~15 minutes:

### 1. 📥 Installation

**[Installation Guide](installation.md)** - Get ddnsd installed

Choose from 3 installation methods:
- **install.sh** (recommended) - One-line installation
- **Pre-built binary** - Download and run
- **Build from source** - For custom builds

**Platform**: Linux amd64 only

### 2. ⚙️ Configuration

**[Configuration Guide](configuration.md)** - Set up your DNS records

**Minimum required**:
```bash
DDNS_PROVIDER_API_TOKEN=your_cloudflare_token
DDNS_RECORDS=example.com,www.example.com
```

**Optional settings**:
- IP source (netlink/http)
- Network interface
- State store type
- Retry behavior

See [Configuration Guide](configuration.md) for complete reference.

### 3. 🚀 Deployment

**[Deployment Guide](deployment.md)** - Start the service

**systemd deployment** (recommended):
- Automatic service startup
- Automatic restart on failure
- Log management via journalctl

**Deployment verification**:
- Check service status
- Verify DNS updates
- Monitor logs

### 4. 🔧 Troubleshooting

**[Troubleshooting Guide](troubleshooting.md)** - If something goes wrong

**Common issues**:
- Daemon won't start
- DNS records not updating
- API token validation failures
- IP detection problems

Each issue includes:
- Symptoms
- Possible causes
- Diagnostic steps
- Solutions

### 5. 🔄 Migration

**[Migration Guide](migration.md)** - Upgrading from v0.1.0

**If upgrading from v0.1.0**:
- No breaking changes
- New environment variables available
- Recommended steps for smooth upgrade

---

## 📋 Prerequisites

**Before you start**, make sure you have:

- ✅ Linux server (amd64) with systemd
- ✅ Cloudflare account with API token
- ✅ Domain managed by Cloudflare
- ✅ Basic familiarity with command line

**Don't have these?** See main [README.md](../../README.md) for details.

---

## ⏱️ Time Investment

| Task | Time |
|------|------|
| Installation | 5 minutes |
| Configuration | 5 minutes |
| Deployment | 3 minutes |
| Verification | 2 minutes |
| **Total** | **~15 minutes** |

---

## 🎯 After Installation

Once ddnsd is running, you'll have:

- ✅ Automatic DNS updates when IP changes
- ✅ Event-driven IP monitoring (Linux Netlink)
- ✅ Crash recovery with state persistence
- ✅ Minimal resource usage (~13 MB RAM)

**Next steps**:
- Monitor logs: `journalctl -u ddnsd -f`
- Configure monitoring: See [Operations Guide](../operations/)
- Customize retry behavior: See [Configuration Guide](configuration.md)

---

## 📞 Need Help?

- **Common issues**: [Troubleshooting Guide](troubleshooting.md)
- **Production operations**: [Operations Guide](../operations/)
- **Report bugs**: [GitHub Issues](https://github.com/ddns-lab/ddns/issues)

---

## 🔗 Related Documentation

- [Architecture](../architecture/) - For contributors and architecture review
- [Security](../security/security.md) - Security best practices
- [Versioning](../meta/versioning.md) - Version compatibility policy
