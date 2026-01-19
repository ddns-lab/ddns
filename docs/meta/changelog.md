# Changelog

All notable changes to the ddns project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [v0.2.2] - 2026-01-19

### Fixed

- **systemctl stop timeout**: Fixed 90-second timeout when stopping service via `systemctl stop ddnsd`
  - Engine now properly handles SIGTERM signals from systemd
  - Added separate handlers for SIGTERM (systemd) and SIGINT (Ctrl-C)
  - Shutdown now completes in ~25ms instead of timing out after 90 seconds
  - Improved state flushing during graceful shutdown

### Changed

- **Environment variable naming**: Provider-specific credentials now use DDNS_ prefix
  - `DDNS_PROVIDER_API_TOKEN` → `DDNS_CLOUDFLARE_API_TOKEN` (Cloudflare)
  - `DDNS_PROVIDER_API_TOKEN` → `DDNS_ALIYUN_ACCESS_KEY_ID` + `DDNS_ALIYUN_ACCESS_KEY_SECRET` (Aliyun)
  - `DDNS_PROVIDER_API_KEY` → `DDNS_NAMESILO_API_KEY` (NameSilo)
  - `DDNS_PROVIDER_API_KEY` → `DDNS_GODADDY_API_KEY` + `DDNS_GODADDY_API_SECRET` (GoDaddy)
- **Install/Uninstall scripts**: Auto-detect non-interactive mode when running via pipe (curl | sh)
- **Configuration template**: Updated install.sh to generate correct provider-specific environment variables

### Docs

- **Migration guide**: Added comprehensive v0.2.0 → v0.2.1 migration instructions with breaking changes
- **Installation guide**: Documented non-interactive mode behavior for pipe installations
- **Updated upgrade paths**: Documented environment variable migration steps

### Technical Details

- Modified `crates/ddns-core/src/engine/mod.rs` to use `tokio::signal::unix::signal()` for SIGTERM/SIGINT
- Updated `install.sh` config template with provider-specific variable names
- Added `! tty -s` detection in install/uninstall scripts for non-interactive mode

---

## [v0.1.2] - 2025-01-15

### Added

- **Comprehensive documentation**: Complete user, operations, and architecture documentation
- **Documentation refactor**: User-centric categorization (user/operations/architecture/meta)
- **Troubleshooting guide**: 10 common issues with solutions
- **Migration guide**: Version upgrade instructions with rollback procedures
- **Monitoring guide**: Log-based monitoring and alerting patterns

### Changed

- **Optimized version references**: Removed hardcoded version numbers to reduce maintenance burden
- **Cleaned up documentation**: Removed duplicate and outdated documents
- **Improved documentation navigation**: Clear paths for users, operators, and contributors

### Removed

- **Duplicate documentation**: INSTALL.md, docs/DEPLOYMENT.md, deploy/README.md
- **Unimplemented features**: Docker and Kubernetes deployment artifacts (v0.2.0)
- **Outdated deployment scripts**: install-systemd.sh (replaced by install.sh)

### Docs

- Added 10 new documentation files
- Reorganized documentation into 5 categories
- Separated user docs from operations docs
- Created clear upgrade paths

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

**Current stable version**: v0.2.2

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
