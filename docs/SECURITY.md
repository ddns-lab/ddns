# Security & Secret Handling Guide

This guide covers security best practices for managing secrets, API tokens, and sensitive configuration in the ddns system.

## Table of Contents

1. [Secret Management](#secret-management)
2. [Configuration Security](#configuration-security)
3. [Secret Validation](#secret-validation)
4. [Secret Rotation](#secret-rotation)
5. [Secret Exposure Prevention](#secret-exposure-prevention)
6. [Auditing and Compliance](#auditing-and-compliance)

---

## Secret Management

### Types of Secrets

| Secret | Purpose | Risk Level | Rotation Frequency |
|--------|---------|------------|-------------------|
| **DNS Provider API Token** | Authenticate with DNS provider API | HIGH | Every 90 days or when compromised |
| **Cloudflare Zone ID** | Identify DNS zone (optional) | LOW | Never (static identifier) |
| **Cloudflare Account ID** | Account-level operations (optional) | MEDIUM | Never (static identifier) |

### Storage Locations

**RECOMMENDED**: Use platform-specific secret management:

| Platform | Secret Storage | Example |
|----------|---------------|---------|
| **systemd** | Environment file with restricted permissions | `/etc/default/ddnsd` (mode 640) |
| **Docker** | Docker secrets or environment variables at runtime | `docker secret create` |
| **Kubernetes** | Kubernetes secrets with RBAC | `kubectl create secret` |
| **Development** | `.env` file (gitignored) | `.env.local` |

**NEVER**:
- ❌ Commit secrets to version control
- ❌ Store in plaintext config files
- ❌ Pass as command-line arguments (visible in `ps`)
- ❌ Include in Docker images
- ❌ Log to console or files

### Platform-Specific Implementations

#### systemd (Linux)

**File**: `/etc/default/ddnsd`

**Permissions**: `640` (root:ddns)

```bash
# Set correct permissions
sudo chown root:ddns /etc/default/ddnsd
sudo chmod 640 /etc/default/ddnsd

# Verify
ls -la /etc/default/ddnsd
# -rw-r----- 1 root ddns ... /etc/default/ddnsd
```

**Why**: Limits secret visibility to root and ddns user only.

---

#### Docker

**Option 1: Environment Variables (Development)**

```bash
docker run -d \
  -e DDNS_PROVIDER_API_TOKEN="$TOKEN" \
  ddnsd:latest
```

**Option 2: Docker Secrets (Production)**

```yaml
# docker-compose.yml
version: '3.8'
services:
  ddnsd:
    image: ddnsd:latest
    secrets:
      - api_token
    environment:
      - DDNS_PROVIDER_API_TOKEN_FILE=/run/secrets/api_token

secrets:
  api_token:
    file: ./secrets/api_token.txt
```

**Option 3: External Secret Store**

```bash
# Use HashiCorp Vault, AWS Secrets Manager, etc.
# Example with Vault:
docker run -d \
  --cap-add=IPC_LOCK \
  -e 'VAULT_ADDR=http://vault:8200' \
  ddnsd:latest
```

---

#### Kubernetes

**Basic Secret**:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: ddnsd-secrets
  namespace: ddns-system
type: Opaque
stringData:
  api-token: "your_token_here"
```

**Sealed Secrets** (Recommended for production):

```bash
# Install Sealed Secrets Controller
kubectl apply -f https://github.com/bitnami-labs/sealed-secrets/releases/download/v0.24.0/controller.yaml

# Create sealed secret (can be committed to git)
kubeseal -f ddnsd-secrets.yaml -o yaml > ddnsd-sealed-secret.yaml
```

**External Secrets Operator** (Enterprise):

```yaml
# Sync secrets from AWS Secrets Manager, Azure Key Vault, etc.
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: ddnsd-secrets
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secrets-manager
    kind: SecretStore
  target:
    name: ddnsd-secrets
  data:
    - secretKey: api-token
      remoteRef:
        key: ddnsd/api-token
```

---

## Configuration Security

### Validation Rules

The daemon implements comprehensive configuration validation to prevent security issues:

#### API Token Validation

```rust
// Validation performed at startup:
✓ Token is not empty
✓ Token length >= 20 characters (Cloudflare is 40)
✓ Token is not a placeholder (your_token, replace_me, example)
```

**Example failures**:

```bash
# Empty token
$ DDNS_PROVIDER_API_TOKEN="" ddnsd
Error: DDNS_PROVIDER_API_TOKEN is required.

# Too short
$ DDNS_PROVIDER_API_TOKEN="abc" ddnsd
Error: DDNS_PROVIDER_API_TOKEN appears too short (3 chars).

# Placeholder
$ DDNS_PROVIDER_API_TOKEN="your_token_here" ddnsd
Error: DDNS_PROVIDER_API_TOKEN appears to be a placeholder.
```

#### Domain Name Validation

```rust
// Per RFC 1035:
✓ Total length <= 253 characters
✓ Each label <= 63 characters
✓ Valid characters: alphanumeric and hyphen
✓ Labels don't start/end with hyphen
✓ No empty labels (consecutive dots)
```

**Example failures**:

```bash
# Too long
$ DDNS_RECORDS="a.very.long.domain.name.that.exceeds.the.maximum.length.allowed.by.dns.standards.and.should.be.shortened.example.com" ddnsd
Error: Domain name too long: 150 chars (max 253).

# Invalid characters
$ DDNS_RECORDS="example_domain.com" ddnsd
Error: Domain label contains invalid characters. Valid: alphanumeric and hyphen only.

# Starts with hyphen
$ DDNS_RECORDS="-example.com" ddnsd
Error: Domain label cannot start or end with hyphen.
```

#### URL Validation

```rust
// For HTTP IP source:
✓ URL is not empty
✓ Uses HTTP or HTTPS scheme
✓ Warning if HTTP instead of HTTPS
```

**Example**:

```bash
# Missing URL
$ DDNS_IP_SOURCE_TYPE=http ddnsd
Error: DDNS_IP_SOURCE_URL is required when DDNS_IP_SOURCE_TYPE=http

# Invalid scheme
$ DDNS_IP_SOURCE_URL="ftp://example.com" ddnsd
Error: DDNS_IP_SOURCE_URL must use HTTP or HTTPS scheme. Got: ftp://example.com

# HTTP warning
$ DDNS_IP_SOURCE_URL="http://api.example.com" ddnsd
WARNING: DDNS_IP_SOURCE_URL uses HTTP (not HTTPS). This is less secure.
```

#### Numeric Range Validation

```rust
// Prevent denial-of-service via extreme values:
✓ DDNS_IP_SOURCE_INTERVAL: 10-3600 seconds
✓ DDNS_MAX_RETRIES: 1-10
✓ DDNS_RETRY_DELAY_SECS: 1-300 seconds
```

**Example failures**:

```bash
# Interval too short
$ DDNS_IP_SOURCE_INTERVAL=1 ddnsd
Error: DDNS_IP_SOURCE_INTERVAL must be between 10 and 3600 seconds. Got: 1

# Too many retries
$ DDNS_MAX_RETRIES=100 ddnsd
Error: DDNS_MAX_RETRIES must be between 1 and 10. Got: 100
```

---

## Secret Validation

### Token Format Validation

**Cloudflare API Tokens**:

```
Format: 40 character alphanumeric string
Example: abcdef1234567890abcdef123456789012345678

Validation:
✓ Length is exactly 40 characters
✓ Contains only alphanumeric characters (a-z, A-Z, 0-9)
```

**If validation fails**:

1. Check you copied the entire token (no truncation)
2. Verify token type (Cloudflare has multiple token types)
3. Ensure token has proper permissions:
   - Zone - DNS - Edit
   - (Optional) Zone - Zone - Read

---

## Secret Rotation

### Rotation Strategy

**Recommended Rotation**: Every 90 days or when compromised

**Rotation Steps**:

1. **Generate new token**:
   ```bash
   # Cloudflare Dashboard
   # 1. Go to: https://dash.cloudflare.com/profile/api-tokens
   # 2. Create new token with same permissions
   # 3. Save new token (old token will still work)
   ```

2. **Update configuration**:
   ```bash
   # systemd
   sudo nano /etc/default/ddnsd
   sudo systemctl restart ddnsd

   # Docker
   docker stop ddnsd && docker rm ddnsd
   DDNS_PROVIDER_API_TOKEN=new_token ./deploy/docker-run.sh

   # Kubernetes
   kubectl create secret generic ddnsd-secrets \
     --from-literal=api-token=new_token \
     --namespace=ddns-system --dry-run=client -o yaml | kubectl apply -f -
   kubectl rollout restart deployment/ddnsd -n ddns-system
   ```

3. **Verify operation**:
   ```bash
   # Check logs for successful DNS updates
   sudo journalctl -u ddnsd -f

   # Look for:
   # "DNS update successful" messages
   # No API authentication errors
   ```

4. **Revoke old token**:
   ```bash
   # Cloudflare Dashboard
   # 1. Go to: https://dash.cloudflare.com/profile/api-tokens
   # 2. Find old token
   # 3. Click "Revoke"
   ```

5. **Document rotation**:
   ```bash
   # Maintain rotation log
   echo "$(date): Rotated API token" >> /var/log/ddns/rotation.log
   ```

---

### Zero-Downtime Rotation

**Kubernetes Rolling Update**:

```yaml
# Strategy: Recreate (not RollingUpdate for single-instance deployment)
spec:
  strategy:
    type: Recreate
```

**Why**: DNS updates are idempotent. A brief downtime during rotation is acceptable since the daemon will retry failed updates on startup.

---

### Rotation Checklist

- [ ] Generate new API token
- [ ] Update secret store (systemd/Docker/Kubernetes)
- [ ] Restart daemon
- [ ] Verify successful DNS updates
- [ ] Check logs for authentication errors
- [ ] Revoke old token
- [ ] Document rotation date
- [ ] Update monitoring/alerting if token expires

---

## Secret Exposure Prevention

### Logging Security

**Daemon logs NEVER contain secrets**:

```rust
// Safe logging:
info!("Starting DNS update for record: {}", record_name);

// NEVER:
info!("Using API token: {}", api_token); // ❌ NEVER DO THIS
```

**Verification**:

```bash
# Check that logs don't contain secrets
sudo journalctl -u ddnsd | grep -i token
# Should return: (no results)

# Safe to check:
sudo journalctl -u ddnsd | grep -i "starting\|update\|error"
```

---

### Process Visibility

**Command-line arguments are visible** (via `ps`):

```bash
# BAD: Secrets in command line
ddnsd --token=your_token_here  # ❌ Visible in ps aux

# GOOD: Secrets via environment
export DDNS_PROVIDER_API_TOKEN=your_token_here
ddnsd  # ✅ Token not in ps aux
```

**Why**: Environment variables are only visible to the process, not to other users via `ps`.

---

### Crash Dumps and Core Files

**Disable core dumps for daemon** (may contain secrets in memory):

**systemd**:
```ini
[Service]
# Core dumps may contain secrets in memory
LimitCORE=0
```

**Docker**:
```bash
# Disable core dumps
docker run --ulimit core=0 ddnsd:latest
```

**Kubernetes**:
```yaml
spec:
  containers:
  - name: ddnsd
    securityContext:
      # Disable core dumps
      resources:
        limits:
          # Prevent memory dumps
    # No volume mounts that could capture memory
```

---

### Backup Security

**Backing up state files**:

```bash
# State file contains IP addresses, NOT API tokens
# But still protect it from unauthorized access

# Backup with encryption
tar -czf - /var/lib/ddns | gpg -e -r your@email.com > ddns-backup.tar.gpg

# Verify no secrets
tar -xzOf ddns-backup.tar.gpg | grep -i token
# Should return: (no results)
```

**What's in state file**:
- ✅ IP addresses
- ✅ Timestamps
- ✅ Record names
- ❌ NO API tokens

---

### Memory Protection

**Runtime memory protection**:

```bash
# Disable ptrace for non-root (prevents gdb/gcore by attackers)
sudo sysctl -w kernel.yama.ptrace_scope=1

# Persist across reboots
echo "kernel.yama.ptrace_scope=1" >> /etc/sysctl.conf
```

**Why**: Prevents attackers from dumping process memory to extract secrets.

---

## Auditing and Compliance

### Audit Logging

**What to audit**:

| Event | Log Level | Example |
|-------|-----------|---------|
| Configuration load | INFO | `"Configuration loaded: 2 record(s)"` |
| Secret validation failure | ERROR | `"API token appears to be a placeholder"` |
| Domain validation failure | ERROR | `"Domain label contains invalid characters"` |
| DNS update attempt | INFO | `"Updating DNS record: example.com"` |
| DNS update success | INFO | `"DNS update successful"` |
| API authentication error | ERROR | `"API call failed: 401 Unauthorized"` |

**Viewing audit logs**:

```bash
# systemd
sudo journalctl -u ddnsd --since "1 hour ago"

# Docker
docker logs ddnsd --since 1h

# Kubernetes
kubectl logs -l app=ddnsd -n ddns-system --since=1h
```

---

### Compliance Considerations

**SOC 2 / ISO 27001**:

- [ ] Secret rotation policy documented (every 90 days)
- [ ] Access control to secret files (640 permissions)
- [ ] Audit logging enabled and retained
- [ ] Secret storage encrypted at rest (disk encryption)
- [ ] Secret transmission encrypted (HTTPS)
- [ ] No secrets in version control
- [ ] Regular security audits

**PCI DSS** (if applicable):

- [ ] API tokens not stored with cardholder data
- [ ] Secret access logged
- [ ] Secret change management process
- [ ] Quarterly secret rotation review

---

### Security Scanning

**Regular secret scans**:

```bash
# Scan for accidental secret commits
# Install trufflehog
brew install trufflehog

# Scan repository
trufflehog git https://github.com/yourorg/ddns.git

# Should return: No secrets found
```

**Pre-commit hooks** (optional):

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Check for API tokens in staged files
if git diff --cached --name-only | xargs grep -q "DDNS_PROVIDER_API_TOKEN.*="; then
    echo "ERROR: Attempting to commit API token!"
    echo "Remove secrets from staged files."
    exit 1
fi
```

---

## Incident Response

### Secret Compromise

**If API token is compromised**:

1. **Immediately revoke token**:
   ```bash
   # Cloudflare Dashboard
   # https://dash.cloudflare.com/profile/api-tokens
   # Click "Revoke" next to compromised token
   ```

2. **Generate new token**:
   - Use different token value (not sequential)
   - Review and narrow token permissions if possible

3. **Rotate all instances**:
   ```bash
   # Update all deployments (dev, staging, prod)
   # Follow rotation procedure in this document
   ```

4. **Investigate compromise**:
   - Check logs for unauthorized API calls
   - Review access logs (who accessed configuration)
   - Scan for exposed secrets in git history

5. **Document incident**:
   - Date/time of discovery
   - Root cause (if determined)
   - Actions taken
   - Prevention measures

---

### Monitoring for Compromise

**Alert on suspicious activity**:

```bash
# Monitor for API authentication errors
sudo journalctl -u ddnsd -f | grep --line-buffered "401 Unauthorized"
# If seen: Possible token compromise

# Monitor for configuration changes
sudo auditctl -w /etc/default/ddnsd -p wa -k ddnsd_config
# Logs to: /var/log/audit/audit.log
```

---

## Summary

**Secret Management Principles**:
1. ✅ Use platform-specific secret storage
2. ✅ Never commit secrets to version control
3. ✅ Rotate tokens every 90 days
4. ✅ Validate all secrets at startup
5. ✅ Never log secrets
6. ✅ Encrypt secrets at rest and in transit
7. ✅ Monitor for secret exposure
8. ✅ Have incident response plan

**Validation Rules**:
- API tokens: >= 20 characters, not placeholder
- Domain names: RFC 1035 compliant
- URLs: HTTP/HTTPS scheme only
- Numeric ranges: Prevent DoS via extreme values

**Rotation Steps**:
1. Generate new token
2. Update configuration
3. Restart daemon
4. Verify operation
5. Revoke old token
6. Document rotation

**Security Checklist**:
- [ ] Secrets not in git
- [ ] File permissions correct (640)
- [ ] Validation enabled
- [ ] Logs contain no secrets
- [ ] Core dumps disabled
- [ ] Regular secret scans
- [ ] Rotation policy in place
- [ ] Incident response plan ready
