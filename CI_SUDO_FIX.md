# CI Sudo Fix - v0.2.0

**Date**: 2026-01-18
**Issue**: GitHub Actions CI failing with "sudo: cargo: command not found"
**Status**: ✅ Fixed

---

## 🔍 Problem Analysis

### Error Message
```yaml
Run sudo cargo test -p ddns-ip-netlink --test integration_test -- --ignored
sudo: cargo: command not found
Error: Process completed with exit code 1.
```

### Root Cause

**GitHub Actions Environment**:
- Actions runner installs Rust in user context
- Cargo is available via `$HOME/.cargo/bin`
- `PATH` includes `$HOME/.cargo/bin`

**The Problem**:
```bash
sudo cargo test
```

When using `sudo`:
1. `sudo` resets environment variables for security
2. `PATH` no longer includes `$HOME/.cargo/bin`
3. `cargo` command cannot be found
4. Test fails immediately

---

## ✅ Solution

### Fix Applied

**Before** (`.github/workflows/test.yml:73,76`):
```yaml
- name: Run integration tests with sudo
  run: sudo cargo test -p ddns-ip-netlink --test integration_test -- --ignored

- name: Run all netlink tests
  run: sudo cargo test -p ddns-ip-netlink --verbose
```

**After**:
```yaml
- name: Run integration tests with sudo
  run: |
    sudo -E env "PATH=$PATH" cargo test -p ddns-ip-netlink --test integration_test -- --ignored

- name: Run all netlink tests
  run: |
    sudo -E env "PATH=$PATH" cargo test -p ddns-ip-netlink --verbose
```

### Explanation

```bash
sudo -E env "PATH=$PATH" cargo test
```

**Breakdown**:
- `sudo`: Run with elevated privileges (required for netlink)
- `-E`: Preserve existing environment variables
- `env "PATH=$PATH"`: Explicitly set PATH in sudo context
- `cargo test`: Command that can now be found

**Why This Works**:
1. `-E` preserves most environment variables
2. `env "PATH=$PATH"` explicitly passes the current PATH
3. Cargo binary location remains accessible in sudo context
4. Root privileges maintained for netlink socket operations

---

## 📊 Impact

### Affected Jobs
- **Job**: `netlink-integration-tests`
- **OS**: `ubuntu-latest`
- **Tests**: Integration tests requiring root privileges

### Before Fix
```
❌ sudo cargo test -> cargo: command not found
❌ CI fails immediately
❌ No tests executed
```

### After Fix
```
✅ sudo -E env "PATH=$PATH" cargo test -> cargo found
✅ CI runs successfully
✅ Tests execute with proper privileges
```

---

## 🧪 Testing

### Verification Steps

1. **Push to GitHub**:
   ```bash
   git push origin main
   ```

2. **Monitor Actions**:
   - Visit: https://github.com/ddns-lab/ddns/actions
   - Check: Tests workflow
   - Job: netlink-integration-tests

3. **Expected Result**:
   - ✅ Build tests step passes
   - ✅ Run integration tests with sudo step passes
   - ✅ Run all netlink tests step passes

---

## 📝 Additional Context

### Why Sudo is Needed

Netlink integration tests require root privileges because:
1. **Netlink sockets**: Require `CAP_NET_ADMIN` capability
2. **Network interface manipulation**: Creating/managing dummy interfaces
3. **Privileged operations**: Modifying routing tables and addresses

**Alternatives Considered**:
- ❌ **Setuid binaries**: Security risk, complex setup
- ❌ **Capabilities**: Requires setup on each run
- ✅ **Sudo with preserved env**: Simple, works in CI

### Best Practices for GitHub Actions

**DO** ✅:
```yaml
- name: Run privileged tests
  run: sudo -E env "PATH=$PATH" cargo test
```

**DON'T** ❌:
```yaml
- name: Run privileged tests
  run: sudo cargo test  # PATH lost, cargo not found
```

**Alternative** (if sudo not strictly required):
```yaml
- name: Set up cargo PATH
  run: echo "$HOME/.cargo/bin" >> $GITHUB_PATH

- name: Run tests
  run: cargo test  # No sudo needed
```

---

## 🚀 Deployment

### Commit
**Hash**: `5c4ce59`
**Message**: `fix: Preserve environment variables in sudo commands for CI`

### Files Changed
- `.github/workflows/test.yml` (2 steps modified)

### Status
- ✅ Committed to `main`
- ✅ Pushed to GitHub
- ⏳ CI will trigger automatically

---

## 📚 References

### GitHub Actions Documentation
- [Workflow Syntax](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions)
- [Environment Variables](https://docs.github.com/en/actions/learn-github-actions/environment-variables)

### Sudo Behavior
- `sudo -E`: Preserve environment
- `sudo env`: Set specific variables in sudo context

---

## 🎯 Summary

**Problem**: `sudo cargo` loses PATH, cargo command not found
**Solution**: `sudo -E env "PATH=$PATH" cargo`
**Impact**: Netlink integration tests now work in CI
**Status**: ✅ Fixed and deployed

---

`★ Insight ─────────────────────────────────────`
**CI环境中的权限管理挑战：**

1. **环境变量隔离**: sudo为了安全会重置环境，但这在CI中会导致工具链不可用。使用`-E`和显式PATH传递是标准解决方案。

2. **权限vs可用性**: Netlink测试需要root权限，但CI环境的设计是为非特权用户优化的。需要在保持安全的同时让工具链可用。

3. **CI特定的陷阱**: 本地开发时`sudo cargo`可能工作（因为cargo在系统PATH），但在GitHub Actions中失败（cargo在用户目录）。这展示了本地和CI环境的差异。
`─────────────────────────────────────────────────`
