# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.3] - 2026-01-20

### Fixed
- Resolve all clippy warnings for unnecessary casts and map_or
- Apply rustfmt formatting fixes
- Remove unused imports from integration tests
- Resolve clippy warnings in netlink integration tests

### Code Quality
- All clippy warnings resolved (`-D warnings` enabled)
- Code 100% rustfmt compliant
- Zero unsafe code in core implementation
- CI/CD fully passing

## [0.2.2] - 2025-01-19

### Added
- GoDaddy DNS provider implementation
- Comprehensive integration test suite
- Real-time netlink event monitoring tests

### Fixed
- Make SIGTERM handling platform-specific
- Fix doctest and unit test compilation errors

### Infrastructure
- Disable Docker integration tests in GitHub Actions

## [0.2.1] - 2025-01-XX

### Added
- NameSilo DNS provider implementation
- HTTP IP source for cross-platform support
- File-based state store implementation

### Fixed
- Improve error handling and retry logic

## [0.2.0] - 2025-01-XX

### Added
- Aliyun DNS provider implementation
- Netlink IP source with event-driven monitoring
- Provider registry plugin system
- Dry-run mode for safe testing

### Changed
- Major refactoring to plugin architecture
- Improved configuration via environment variables

## [0.1.2] - 2024-XX-XX

### Added
- Initial Cloudflare provider implementation
- Basic IP monitoring
- State management

## [0.1.1] - 2024-XX-XX

### Fixed
- Bug fixes and stability improvements

## [0.1.0] - 2024-XX-XX

### Added
- Initial release
- Core architecture and trait definitions
- Basic DDNS functionality

[0.2.3]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.3
[0.2.2]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.2
[0.2.1]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.1
[0.2.0]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.0
[0.1.2]: https://github.com/ddns-lab/ddns/releases/tag/v0.1.2
[0.1.1]: https://github.com/ddns-lab/ddns/releases/tag/v0.1.1
[0.1.0]: https://github.com/ddns-lab/ddns/releases/tag/v0.1.0
