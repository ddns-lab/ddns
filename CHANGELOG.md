# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.5] - 2026-08-02

### Fixed
- Cloudflare provider now updates DNS records via PATCH with only `content`,
  preserving the `proxied` (orange/grey cloud) setting. The previous PUT-based
  implementation reset orange-cloud records to DNS-only on every update.
- Align `aliyun`, `godaddy`, and `namesilo` provider crates with workspace
  inheritance (`version`/`edition`/`license`/`authors`/`repository.workspace =
  true`, shared deps via `{ workspace = true }`), matching the `cloudflare` crate.

### Added
- `CloudflareProvider::new_with_base()` for injecting the API base URL
  (testability).
- wiremock regression test asserting the update uses PATCH with a content-only
  body, guarding against reintroducing the proxied-reset bug.

## [0.2.4] - 2026-04-04

### Fixed
- Preserve Cloudflare `proxied` (orange cloud) setting during DNS record updates
- Engine now fails immediately on authentication errors instead of wasting retries
  (prevents cascading 429 rate limits from repeated 401 attempts)

### Added
- `Error::is_retryable()` to classify errors as recoverable vs non-recoverable
- Map Cloudflare 401/403 to `Error::Authentication` and 429 to `Error::RateLimited`

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

## [0.2.2] - 2026-01-19

### Added
- GoDaddy DNS provider implementation
- Comprehensive integration test suite
- Real-time netlink event monitoring tests

### Fixed
- Make SIGTERM handling platform-specific
- Fix doctest and unit test compilation errors

### Infrastructure
- Disable Docker integration tests in GitHub Actions

## [0.2.1] - 2026-01-19

### Added
- NameSilo DNS provider implementation
- HTTP IP source for cross-platform support
- File-based state store implementation

### Fixed
- Improve error handling and retry logic

## [0.2.0] - 2026-01-18

### Added
- Aliyun DNS provider implementation
- Netlink IP source with event-driven monitoring
- Provider registry plugin system
- Dry-run mode for safe testing

### Changed
- Major refactoring to plugin architecture
- Improved configuration via environment variables

## [0.1.2] - 2026-01-15

### Added
- Initial Cloudflare provider implementation
- Basic IP monitoring
- State management

## [0.1.1] - 2026-01-13

### Fixed
- Bug fixes and stability improvements

## [0.1.0] - 2026-01-13

### Added
- Initial release
- Core architecture and trait definitions
- Basic DDNS functionality

[0.2.5]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.5
[0.2.4]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.4
[0.2.3]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.3
[0.2.2]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.2
[0.2.1]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.1
[0.2.0]: https://github.com/ddns-lab/ddns/releases/tag/v0.2.0
[0.1.2]: https://github.com/ddns-lab/ddns/releases/tag/v0.1.2
[0.1.1]: https://github.com/ddns-lab/ddns/releases/tag/v0.1.1
[0.1.0]: https://github.com/ddns-lab/ddns/releases/tag/v0.1.0
