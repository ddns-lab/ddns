# Changelog

All notable changes to the ddns project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [v0.1.1] - 2025-01-14

### Added

- **Auto-create DNS records**: Records are automatically created in Cloudflare if they don't exist
- **Better error messages**: More specific, actionable error messages for API failures
- **Enhanced logging**: Improved IP change detection logs, clearer startup messages
- **Startup delay option**: New `DDNS_STARTUP_DELAY_SECS` environment variable
- **Minimum update interval**: New `DDNS_MIN_UPDATE_INTERVAL_SECS` environment variable

### Changed

- Improved IP source validation with better interface name handling
- Enhanced state file error handling and recovery
- Refined logging format for easier parsing

### Fixed

- **Cloudflare API 400 error**: Fixed missing 'name' field in PUT request for record updates
- **Daemon crash after 30 seconds**: Removed timeout on shutdown wait that caused premature termination
- **Initial DNS update not triggered**: Fixed bug where first update wasn't sent on startup
- **Interface name parsing**: Fixed parsing of veth interface names (e.g., `eth0@if14`)
- **API token validation**: Added 40-character length check for Cloudflare tokens

### Security

- No security issues in this release

---

## [v0.1.0] - 2025-01-13

### Added

- **Initial release of ddns**
- **Event-driven architecture**: React to IP changes instantly via Linux Netlink
- **IP Sources**:
  - Netlink IP source for Linux (RTM_NEWADDR/RTM_DELADDR events)
  - HTTP polling IP source for non-Linux platforms
- **DNS Providers**:
  - Cloudflare provider with A/AAAA record support
  - Automatic zone discovery
- **State Management**:
  - File-based state store with atomic writes
  - In-memory state store for testing
  - Idempotency via state tracking
- **Retry Logic**: Exponential backoff for failed API calls
- **Configuration**: Environment variable-based configuration (no config files)
- **Deployment**:
  - Systemd integration
  - One-line installer script
  - Auto-start on boot
- **Testing**: Comprehensive architectural contract tests
- **CI/CD**: GitHub Actions for testing, linting, security auditing

---

## [Unreleased]

### Planned for Future Releases

- Additional DNS providers (Route53, DigitalOcean, Namecheap)
- macOS/Windows native IP sources
- Docker and Kubernetes deployment
- Prometheus metrics endpoint
- Health check HTTP endpoint
- Web UI for monitoring and configuration
- Configuration profiles for multiple providers

---

## Types of Changes

- **Added**: New features
- **Changed**: Changes to existing functionality
- **Deprecated**: Soon-to-be removed features
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security vulnerability fixes

---

## Versioning Policy

This project follows [Semantic Versioning](versioning.md):

- **MAJOR**: Incompatible API changes
- **MINOR**: Backward-compatible functionality additions
- **PATCH**: Backward-compatible bug fixes

**Current stable version**: v0.1.1

**Minimum supported version**: v0.1.0

---

## Upgrade Notes

### v0.1.0 → v0.1.1

**Breaking changes**: None

**Required actions**:
1. Backup configuration file: `sudo cp /etc/ddnsd/ddnsd.env /etc/ddnsd/ddnsd.env.backup`
2. Run upgrade script: `curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh`
3. Verify upgrade: `ddnsd --version` (should show v0.1.1)
4. Check logs: `sudo journalctl -u ddnsd -n 20`

**Optional new variables** (not required):
- `DDNS_STARTUP_DELAY_SECS`: Delay before monitoring starts
- `DDNS_MIN_UPDATE_INTERVAL_SECS`: Minimum time between DNS updates

For detailed upgrade instructions, see [Migration Guide](../user/migration.md).

---

## Links

- [Current Version](https://github.com/ddns-lab/ddns/releases/latest)
- [All Releases](https://github.com/ddns-lab/ddns/releases)
- [Migration Guide](../user/migration.md)
- [Versioning Policy](versioning.md)
