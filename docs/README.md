# 📚 Documentation

Welcome to the ddns documentation. This guide helps you find the right documentation based on your role and needs.

## 🚀 Quick Start

**New to ddns?** Start with [📖 User Guide](user/)

**Running in production?** See [🔧 Operations Guide](operations/)

**Contributing?** See [🏗️ Architecture](architecture/)

---

## 📂 Documentation by Category

### 👤 User Documentation

**For users who want to install, configure, and run ddnsd**

- [User Guide](user/) - Complete user documentation index
  - [Installation](user/installation.md) - 3 ways to install ddnsd
  - [Configuration](user/configuration.md) - Complete environment variable reference
  - [Deployment](user/deployment.md) - systemd deployment and verification
  - [Troubleshooting](user/troubleshooting.md) - Common issues and solutions
  - [Migration](user/migration.md) - Version upgrade guides

### 🔧 Operations Documentation

**For operators running ddnsd in production**

- [Operations Guide](operations/) - Production operations documentation
  - [Crash Recovery](operations/crash-recovery.md) - State corruption and recovery
  - [Operations](operations/ops.md) - Signal handling and process lifecycle
  - [Observability](operations/observability.md) - Logging, metrics, health checks
  - [Secret Rotation](operations/secret-rotation.md) - API token rotation
  - [Monitoring](operations/monitoring.md) - Monitoring integration

### 🏗️ Architecture Documentation

**For contributors and architecture reviewers**

- [Architecture](architecture/) - System design and boundaries
  - [ARCHITECTURE.md](architecture/ARCHITECTURE.md) - System overview
  - [CONFIGURATION.md](architecture/CONFIGURATION.md) - Configuration contract
  - [FAILURE_MODEL.md](architecture/FAILURE_MODEL.md) - Error handling model
  - [LIFECYCLE.md](architecture/LIFECYCLE.md) - Startup and shutdown semantics
  - [PERFORMANCE.md](architecture/PERFORMANCE.md) - Performance characteristics
  - [TRAIT_BOUNDARIES.md](architecture/TRAIT_BOUNDARIES.md) - Extension points
  - [TRUST_LEVELS.md](architecture/TRUST_LEVELS.md) - Trust levels and security boundaries

### ✅ Validation Documentation

**For security review and production acceptance**

- [Cloudflare Provider Validation](validation/cloudflare-provider.md) - Real-world testing results

### 🔒 Security Documentation

**For security audits and compliance**

- [Security Guide](security/security.md) - Key management and security best practices

### 📋 Meta Documentation

**For release management and version planning**

- [Versioning](meta/versioning.md) - Semantic versioning policy
- [Changelog](meta/changelog.md) - Complete change history

---

## 🎯 Finding What You Need

### "I'm a new user..."
**Start here**: [User Guide](user/)

**What you'll learn**:
- How to install ddnsd (3 methods)
- How to configure DNS records
- How to deploy with systemd
- How to troubleshoot common issues

**Time investment**: ~15 minutes

### "I'm an operator..."
**Start here**: [Operations Guide](operations/)

**What you'll learn**:
- How to monitor ddnsd (logs, metrics)
- How to recover from crashes
- How to rotate API tokens
- How to handle process lifecycle

**Time investment**: ~20 minutes

### "I want to contribute..."
**Start here**: [Architecture](architecture/ARCHITECTURE.md)

**What you'll learn**:
- System design and boundaries
- Extension points (traits)
- How to add new providers
- How to add new IP sources

**⚠️ Important**: Before contributing, read [.ai/AI_CONTRACT.md](../.ai/AI_CONTRACT.md) for architectural constraints.

**Time investment**: ~30 minutes

---

## 📋 Platform Limitations

**ddns v0.1.1 supports**:
- ✅ Linux (amd64)
- ✅ Cloudflare DNS provider
- ✅ systemd deployment
- ✅ Environment variable configuration

**Not yet supported** (planned for future releases):
- ❌ Windows/macOS
- ❌ Docker/Kubernetes deployment (planned for v0.2.0)
- ❌ Other DNS providers
- ❌ Configuration files

See [README.md](../README.md) "Upcoming Features" for roadmap.

---

## 📞 Getting Help

- **Troubleshooting**: [User Troubleshooting](user/troubleshooting.md)
- **Issues**: [GitHub Issues](https://github.com/ddns-lab/ddns/issues)
- **Architecture**: [Architecture Documentation](architecture/)
