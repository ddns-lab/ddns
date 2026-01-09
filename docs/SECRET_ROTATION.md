# Secret Rotation Quick Reference

Quick guide for rotating DNS provider API tokens across different deployment platforms.

## Rotation Prerequisites

Before rotating, ensure you have:
- [ ] Access to DNS provider dashboard (Cloudflare, etc.)
- [ ] Permissions to create new API tokens
- [ ] Access to deployment platform (systemd/Docker/Kubernetes)
- [ ] Understanding of rotation steps below

---

## Platform-Specific Procedures

### systemd (Linux)

**Step 1: Generate new API token**

1. Go to: https://dash.cloudflare.com/profile/api-tokens
2. Click "Create Token"
3. Use template: "Edit zone DNS"
4. Select zones/resources as needed
5. Click "Continue to summary" → "Create Token"
6. **Copy new token immediately** (won't be shown again)

**Step 2: Update configuration**

```bash
# Edit environment file
sudo nano /etc/default/ddnsd

# Update token line:
DDNS_PROVIDER_API_TOKEN=new_40_character_token_here

# Save and exit (Ctrl+X, Y, Enter)
```

**Step 3: Restart daemon**

```bash
# Restart daemon to load new token
sudo systemctl restart ddnsd

# Verify it started
sudo systemctl status ddnsd
```

**Step 4: Verify operation**

```bash
# Check logs for successful startup
sudo journalctl -u ddnsd -n 20

# Should see:
# "Starting ddnsd daemon"
# "Configuration loaded"
# "Ready to monitor IP changes"
```

**Step 5: Revoke old token**

1. Go to: https://dash.cloudflare.com/profile/api-tokens
2. Find old token in list
3. Click "Revoke"
4. Confirm revocation

**Step 6: Document rotation**

```bash
# Log rotation date
echo "$(date '+%Y-%m-%d'): API token rotated" | sudo tee -a /var/log/ddns/rotation.log
```

---

### Docker

**Step 1: Generate new API token**

(Same as systemd Step 1 above)

**Step 2: Update environment variable**

```bash
# Set new token
export DDNS_PROVIDER_API_TOKEN=new_40_character_token_here

# Verify it's set
echo $DDNS_PROVIDER_API_TOKEN
```

**Step 3: Rebuild and redeploy**

```bash
# If using docker-run script
DDNS_PROVIDER_API_TOKEN=new_token \
DDNS_RECORDS=example.com \
./deploy/docker-run.sh

# If using docker-compose
# Update docker-compose.override.yml:
# environment:
#   - DDNS_PROVIDER_API_TOKEN=new_token
docker-compose down
docker-compose up -d
```

**Step 4: Verify operation**

```bash
# Check logs
docker logs -f ddnsd

# Should see successful startup
```

**Step 5: Revoke old token**

(Same as systemd Step 5 above)

**Step 6: Document rotation**

```bash
# Log rotation
echo "$(date '+%Y-%m-%d'): API token rotated (Docker)" >> docker-rotation.log
```

---

### Kubernetes

**Step 1: Generate new API token**

(Same as systemd Step 1 above)

**Step 2: Update secret**

```bash
# Update secret with new token
kubectl create secret generic ddnsd-secrets \
  --from-literal=api-token=new_40_character_token_here \
  --namespace=ddns-system \
  --dry-run=client -o yaml | kubectl apply -f -

# Verify secret updated
kubectl get secret ddnsd-secrets -n ddns-system -o yaml
```

**Step 3: Rollout restart**

```bash
# Restart deployment to load new secret
kubectl rollout restart deployment/ddnsd -n ddns-system

# Watch rollout status
kubectl rollout status deployment/ddnsd -n ddns-system
```

**Step 4: Verify operation**

```bash
# Check new pod logs
kubectl logs -l app=ddnsd -n ddns-system --tail=20

# Should see successful startup
```

**Step 5: Revoke old token**

(Same as systemd Step 5 above)

**Step 6: Document rotation**

```bash
# Log rotation in cluster
kubectl create configmap rotation-log \
  --from-literal=last-rotation="$(date '+%Y-%m-%d')" \
  --namespace=ddns-system \
  --dry-run=client -o yaml | kubectl apply -f -
```

---

## Verification Checklist

After rotation, verify:

- [ ] Daemon started successfully (exit code 0)
- [ ] Logs show "Ready to monitor IP changes"
- [ ] No authentication errors in logs
- [ ] DNS update test successful (if IP changes)
- [ ] Old token revoked
- [ ] Rotation date documented

---

## Rollback Procedure

If new token fails:

1. **Don't revoke old token yet**
2. **Investigate failure**:
   ```bash
   # Check logs for error
   sudo journalctl -u ddnsd -n 50  # systemd
   docker logs ddnsd               # Docker
   kubectl logs -l app=ddnsd -n ddns-system  # Kubernetes
   ```

3. **Revert to old token**:
   ```bash
   # systemd
   sudo nano /etc/default/ddnsd  # Change back to old token
   sudo systemctl restart ddnsd

   # Docker
   DDNS_PROVIDER_API_TOKEN=old_token ./deploy/docker-run.sh

   # Kubernetes
   kubectl create secret generic ddnsd-secrets \
     --from-literal=api-token=old_token \
     --namespace=ddns-system \
     --dry-run=client -o yaml | kubectl apply -f -
   kubectl rollout restart deployment/ddnsd -n ddns-system
   ```

4. **Investigate new token issue**:
   - Verify token permissions
   - Check token hasn't expired
   - Ensure token copied correctly (no extra spaces)
   - Confirm correct zone selected in token permissions

---

## Common Issues

### Issue: "401 Unauthorized" after rotation

**Cause**: Token invalid or insufficient permissions

**Solution**:
1. Verify token copied correctly (40 characters, no spaces)
2. Check token permissions in Cloudflare dashboard
3. Ensure token includes correct zone/account

### Issue: "Zone not found" after rotation

**Cause**: New token missing zone permissions

**Solution**:
1. Go to Cloudflare dashboard
2. Edit token permissions
3. Ensure correct zones selected
4. Or add `DDNS_PROVIDER_ZONE_ID` to config

### Issue: Old token already revoked

**If old token already revoked and new token doesn't work**:

1. **Create another new token** (third token)
2. Follow rotation procedure
3. Investigate why second token failed
4. Contact provider support if needed

---

## Automation Options

### Automated Rotation (Optional)

For enterprise environments, consider automated rotation:

**HashiCorp Vault**:
```bash
# Vault generates and rotates tokens automatically
# Daemon fetches current token from Vault on startup
```

**External Secrets Operator** (Kubernetes):
```yaml
# Syncs secrets from AWS Secrets Manager, Azure Key Vault, etc.
# Automatically updates secrets and restarts deployments
```

**Custom rotation script**:
```bash
#!/bin/bash
# rotate-token.sh
# 1. Generate new token via Cloudflare API
# 2. Update systemd/Docker/Kubernetes secret
# 3. Restart daemon
# 4. Verify operation
# 5. Revoke old token
# 6. Document rotation
```

**Warning**: Automation adds complexity. Manual rotation is simpler and safer for most deployments.

---

## Rotation Schedule

**Recommended**: Every 90 days

**Why**:
- Reduces risk window if token is compromised
- Security best practice (compliance requirements)
- Cloudflare tokens don't expire, so manual rotation required

**Calendar reminder**:
```bash
# Add to crontab to alert every 85 days
# At 9am on day 85 of every 2nd month:
0 9 */85 * * echo "API token rotation due in 5 days" | mail -s "ddnsd token reminder" admin@example.com
```

---

## Contact Information

**For issues**:
- Cloudflare Support: https://support.cloudflare.com/
- GitHub Issues: https://github.com/ddns-lab/ddns/issues
- Documentation: See `docs/SECURITY.md` for detailed security guide

---

## Quick Reference Commands

| Platform | Restart Command | Log Command |
|----------|-----------------|-------------|
| **systemd** | `sudo systemctl restart ddnsd` | `sudo journalctl -u ddnsd -f` |
| **Docker** | `docker restart ddnsd` | `docker logs -f ddnsd` |
| **Kubernetes** | `kubectl rollout restart deployment/ddnsd -n ddns-system` | `kubectl logs -f deployment/ddnsd -n ddns-system` |

---

**Last Updated**: 2025-01-09
**Next Review**: 2025-04-09 (90 days)
