# Versioning & Compatibility Contract

This document defines the versioning policy and compatibility guarantees for the ddns project.

## Versioning Policy

The ddns project follows **Semantic Versioning 2.0.0** (SemVer).

### Version Format

```
MAJOR.MINOR.PATCH
```

- **MAJOR**: Incompatible API changes
- **MINOR**: Backward-compatible functionality additions
- **PATCH**: Backward-compatible bug fixes

### Current Version

**Workspace Version**: `0.1.0`

**Version Status**: Pre-release (0.x)
- Minor versions (0.1, 0.2) may include breaking changes
- Version 1.0.0 will signal stable API guarantees
- Until 1.0.0, treat minor version changes as potentially breaking

---

## Stability Levels

### Stable APIs

These APIs have **backward compatibility guarantees** within a MAJOR version:

| API | Stability | Breaking Changes Require |
|-----|-----------|-------------------------|
| `IpSource` trait | Stable | MAJOR version bump |
| `DnsProvider` trait | Stable | MAJOR version bump |
| `StateStore` trait | Stable | MAJOR version bump |
| `DnsEngine` public methods | Stable | MAJOR version bump |
| `ProviderRegistry` public methods | Stable | MAJOR version bump |
| Configuration enums (`*Config`) | Stable | MAJOR version bump |
| Error variants | Stable | MAJOR version bump |
| `StateRecord` structure | Stable | MAJOR version bump |

### Internal APIs

These are **implementation details** and may change without notice:

- Internal engine fields and methods
- Event emission internals
- Registry internal data structures
- Trait method implementations (not signatures)

---

## Breaking Change Criteria

### What Requires a MAJOR Version Bump

#### 1. Trait Method Changes

**Breaking**:
```rust
// BEFORE
trait DnsProvider {
    async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult>;
}

// AFTER (new parameter - BREAKING)
trait DnsProvider {
    async fn update_record(&self, record: &str, ip: IpAddr, ttl: Option<u32>) -> Result<UpdateResult>;
}
```

**Not Breaking**:
```rust
// Adding new trait method with default implementation
trait DnsProvider {
    async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult>;

    // NEW METHOD with default
    async fn validate_record(&self, record: &str) -> Result<bool> {
        Ok(true)
    }
}
```

#### 2. Configuration Enum Changes

**Breaking**:
```rust
// BEFORE
pub enum ProviderConfig {
    Cloudflare { api_token: String, zone_id: Option<String> },
}

// AFTER (removed variant - BREAKING)
pub enum ProviderConfig {
    Route53 { access_key: String, secret_key: String },
}
```

**Not Breaking**:
```rust
// Adding new variant
pub enum ProviderConfig {
    Cloudflare { api_token: String, zone_id: Option<String> },
    Route53 { access_key: String, secret_key: String },  // NEW
}
```

**Not Breaking**:
```rust
// Adding optional fields
pub enum ProviderConfig {
    Cloudflare {
        api_token: String,
        zone_id: Option<String>,
        account_id: Option<String>,  // NEW with #[serde(default)]
    },
}
```

#### 3. Error Type Changes

**Breaking**:
```rust
// Removing error variant
pub enum Error {
    DnsProvider(String),  // REMOVED
}

// Renaming error variant
pub enum Error {
    ProviderError(String),  // was DnsProvider
}
```

**Not Breaking**:
```rust
// Adding new error variant
pub enum Error {
    DnsProvider(String),
    RateLimited(String),  // NEW
}
```

#### 4. State Format Changes

**Breaking**:
```rust
// BEFORE
pub struct StateRecord {
    pub last_ip: IpAddr,
    pub last_updated: DateTime<Utc>,
}

// AFTER (removed field - BREAKING)
pub struct StateRecord {
    pub last_ip: IpAddr,
    pub last_updated: DateTime<Utc>,
    pub checksum: String,  // NEW required field
}
```

**Not Breaking**:
```rust
// Adding optional field
pub struct StateRecord {
    pub last_ip: IpAddr,
    pub last_updated: DateTime<Utc>,
    pub checksum: Option<String>,  // NEW optional
}
```

### What Requires a MINOR Version Bump

- Adding new trait methods (with default implementations)
- Adding new configuration variants
- Adding new error variants
- Adding optional fields to structs
- Adding new public APIs (functions, structs, enums)

### What Requires a PATCH Version Bump

- Bug fixes that don't change API
- Performance improvements
- Documentation updates
- Internal refactoring (no API changes)

---

## Provider Compatibility

### Provider Crate Dependencies

Provider crates **must** specify dependency constraints:

```toml
# ddns-provider-cloudflare/Cargo.toml
[dependencies]
ddns-core = { path = "../ddns-core", version = "0.1" }

# Use tilde requirement for compatible updates:
ddns-core = { version = "0.1", path = "../ddns-core" }

# Or caret requirement (recommended):
ddns-core = { version = "^0.1.0", path = "../ddns-core" }
```

### Compatibility Matrix

| ddns-core Version | Provider Compatible With | Notes |
|-------------------|-------------------------|-------|
| 0.1.0 | 0.1.x | Compatible within patch versions |
| 0.2.0 | 0.1.x | May be incompatible (check release notes) |
| 1.0.0 | 0.1.x | Incompatible (trait changes likely) |

### Provider Testing Checklist

When ddns-core releases a new version, provider maintainers should verify:

1. **Trait Compilation**: Do trait implementations still compile?
2. **Configuration**: Do config enums match expected variants?
3. **Error Handling**: Do error patterns still work?
4. **State Format**: Can old state be loaded?
5. **Integration Tests**: Do all tests pass?

---

## Migration Guide

### For Provider Authors

#### Upgrading from ddns-core 0.1.x to 0.2.0

**Step 1**: Check breaking changes in CHANGELOG.md
**Step 2**: Update Cargo.toml version constraint
**Step 3**: Recompile and fix trait implementations
**Step 4**: Test configuration loading
**Step 5**: Test state file loading (if applicable)

#### Handling Breaking Changes

If ddns-core makes a breaking change you need:

**Option 1**: Pin to old version
```toml
[dependencies]
ddns-core = { version = "=0.1.0", path = "../ddns-core" }
```

**Option 2**: Fork and maintain compatibility
```toml
[dependencies]
ddns-core = { git = "https://github.com/yourfork/ddns", branch = "compat-0.1" }
```

**Option 3**: Update implementation (recommended)
- Update trait implementations
- Adjust configuration handling
- Update state migration logic

---

## State File Migration

### State File Versioning

State files include a version field for future compatibility:

```rust
// State file format (JSON):
{
  "version": "1.0",
  "records": {
    "example.com": {
      "last_ip": "1.2.3.4",
      "last_updated": "2025-01-09T12:00:00Z",
      "provider_metadata": {}
    }
  }
}
```

### Migration Strategy

When state format changes:

1. **Add migration utility** in ddns-core
2. **Support multiple versions** during transition
3. **Auto-migrate on load**
4. **Deprecate old format** after one major version

**Example**:
```rust
impl FileStateStore {
    async fn load_state(&mut self) -> Result<State> {
        let raw = fs::read_to_string(&self.path).await?;

        // Try current format
        if let Ok(state) = serde_json::from_str::<StateV2>(&raw) {
            return Ok(state.into());
        }

        // Try old format and migrate
        if let Ok(state) = serde_json::from_str::<StateV1>(&raw) {
            warn!("Migrating state file from v1 to v2");
            let migrated = state.migrate_to_v2();
            self.save_state(&migrated).await?;
            return Ok(migrated);
        }

        Err(Error::config("Unknown state file format"))
    }
}
```

---

## Release Process

### Pre-Release Checklist

Before releasing a new version:

1. **Update CHANGELOG.md** with all changes
2. **Run full test suite**: `cargo test --all-features`
3. **Test provider compatibility**: Build and test at least one provider
4. **Check breaking changes**: Verify MAJOR version bump if needed
5. **Update documentation**: Update VERSIONING.md if policies change
6. **Tag release**: `git tag -a v0.2.0 -m "Release v0.2.0"`
7. **Push tags**: `git push origin v0.2.0`

### Version Bump Examples

**PATCH Release** (bug fix):
```bash
# 0.1.0 → 0.1.1
# Change: Fixed rate limiting bug
# Breaking: No
```

**MINOR Release** (new feature):
```bash
# 0.1.1 → 0.2.0
# Change: Added HTTP IP source
# Breaking: No (added new enum variant)
```

**MAJOR Release** (breaking change):
```bash
# 0.2.0 → 1.0.0
# Change: Redesigned DnsProvider trait
# Breaking: Yes (trait method signature changed)
```

---

## Stability Guarantees (Post-1.0)

Once version **1.0.0** is released:

### Guaranteed Stable

- Trait method signatures will not change
- Configuration enum variants will not be removed
- Error variants will not be removed (new variants may be added)
- StateRecord will maintain backward-compatible serialization

### May Change Without MAJOR Bump

- Internal implementation details
- Default values for optional fields
- Error messages (not variant types)
- Performance characteristics
- Logging format and verbosity

### Require Explicit Migration

- State file format changes
- Configuration format changes
- Trait method additions (without default implementations)

---

## Deprecation Policy

### Deprecation Process

When deprecating an API:

1. **Release N+0**: Mark as deprecated, document replacement
2. **Release N+1**: Update documentation, add compiler warnings if possible
3. **Release N+2**: Remove deprecated API (MAJOR version bump)

**Example**:
```rust
/// # Deprecated
///
/// This method is deprecated since 0.2.0.
/// Use `update_record_with_ttl()` instead.
///
/// Will be removed in version 1.0.0.
#[deprecated(since = "0.2.0", note = "Use `update_record_with_ttl()` instead")]
async fn update_record(&self, record: &str, ip: IpAddr) -> Result<UpdateResult> {
    self.update_record_with_ttl(record, ip, None).await
}
```

---

## FAQ

### Q: Can I use ddns-core 0.2.x with a provider built for 0.1.x?

**A**: No. Provider crates must be recompiled against the ddns-core version they depend on. If ddns-core 0.2.0 has breaking changes, providers need to be updated.

### Q: How long will you support ddns-core 0.1.x after 1.0.0 is released?

**A**: We will maintain bugfix support for the latest 0.x release for 6 months after 1.0.0. After that, users are encouraged to upgrade to 1.x.

### Q: Will my state files work after upgrading?

**A**: Yes, for MINOR and PATCH versions. For MAJOR versions, we provide migration utilities. State file format is part of the stable API.

### Q: Can I rely on specific error messages?

**A**: No. Error messages may change without notice. Rely on error **types** (variants), not messages.

### Q: What if I need a feature that requires a breaking change?

**A**: We'll evaluate the request based on:
- Impact on existing providers
- Migration complexity
- Architectural alignment
If accepted, we'll release as MAJOR version with migration guide.

---

## Summary

**Versioning**: Semantic Versioning (SemVer 2.0.0)
**Current Version**: 0.1.0 (pre-release)
**Stable APIs**: Traits, Configs, Errors, StateRecord
**Breaking Changes**: Require MAJOR version bump
**Provider Compatibility**: Version constraints required

**Key Principle**: If it breaks existing providers or state files, it's a MAJOR version change.
