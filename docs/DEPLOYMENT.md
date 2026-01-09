# Deployment Guide

This guide covers deploying the ddns daemon in various environments: systemd (Linux), Docker, and Kubernetes.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Prerequisites](#prerequisites)
3. [Deployment Options](#deployment-options)
4. [Configuration](#configuration)
5. [Security Considerations](#security-considerations)
6. [Monitoring & Troubleshooting](#monitoring--troubleshooting)
7. [Upgrading](#upgrading)

---

## Quick Start

### systemd (Linux)

```bash
# Build
cargo build --release

# Install
sudo ./deploy/install-systemd.sh

# Configure
sudo nano /etc/default/ddnsd

# Start
sudo systemctl enable ddnsd
sudo systemctl start ddnsd
```

### Docker

```bash
# Build
docker build -t ddnsd:latest .

# Run
DDNS_PROVIDER_API_TOKEN=your_token \
DDNS_RECORDS=example.com,www.example.com \
./deploy/docker-run.sh
```

### Kubernetes

```bash
# Build and push image
docker build -t ddnsd:latest .
# docker tag ddnsd:latest registry.example.com/ddnsd:latest
# docker push registry.example.com/ddnsd:latest

# Deploy
./deploy/k8s-deploy.sh
```

---

## Prerequisites

### Build Requirements

- Rust 1.83+ with Cargo
- For netlink IP source (Linux): `libnetfilter_queue-dev` or equivalent
- For Cloudflare provider: None (uses `tokio` for HTTP)

### Runtime Requirements

- Linux (recommended) or other Unix-like OS
- Network connectivity for DNS provider APIs
- (Optional) Linux Netlink access for IP monitoring

### Platform-Specific Notes

| Platform | IP Source | Notes |
|----------|-----------|-------|
| **Linux** | Netlink (recommended) | Uses kernel Netlink for real-time IP changes |
| **macOS** | HTTP polling | Netlink not available, use HTTP IP source |
| **Windows** | HTTP polling | Not tested, may require WSL |
| **FreeBSD** | HTTP polling | Netlink not available, use HTTP IP source |

---

## Deployment Options

### 1. systemd (Linux)

**Best for**: Traditional servers, VPS, bare metal

**Advantages**:
- Native process supervision
- Automatic restart on failure
- Integrated logging (journal)
- Boot-time startup

**Installation**:

See `deploy/install-systemd.sh` for automated installation.

**Manual Installation**:

```bash
# Create user
sudo useradd -r -s /bin/false -d /var/lib/ddns ddns

# Create directories
sudo mkdir -p /var/lib/ddns /var/log/ddns
sudo chown -R ddns:ddns /var/lib/ddns /var/log/ddns

# Install binary
sudo cp target/release/ddnsd /usr/local/bin/
sudo chmod 755 /usr/local/bin/ddnsd

# Install service files
sudo cp deploy/ddnsd.service /etc/systemd/system/
sudo cp deploy/ddnsd.default /etc/default/ddnsd

# Edit configuration
sudo nano /etc/default/ddnsd

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable ddnsd
sudo systemctl start ddnsd
```

**Configuration**:

Edit `/etc/default/ddnsd`:

```bash
DDNS_IP_SOURCE_TYPE=netlink
DDNS_IP_SOURCE_INTERFACE=eth0
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=your_token_here
DDNS_RECORDS=example.com,www.example.com
DDNS_STATE_STORE_TYPE=file
DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
```

**Management**:

```bash
# Check status
sudo systemctl status ddnsd

# View logs
sudo journalctl -u ddnsd -f

# Restart
sudo systemctl restart ddnsd

# Stop
sudo systemctl stop ddnsd

# Reload configuration
sudo systemctl edit ddnsd  # Edit environment
sudo systemctl restart ddnsd
```

---

### 2. Docker

**Best for**: Containerized environments, testing, edge devices

**Advantages**:
- Isolated runtime environment
- Consistent across platforms
- Easy rollback
- Resource limits

**Building**:

```bash
docker build -t ddnsd:latest .
```

**Running**:

**Option 1: Using docker-run script**

```bash
DDNS_PROVIDER_API_TOKEN=your_token \
DDNS_RECORDS=example.com,www.example.com \
./deploy/docker-run.sh
```

**Option 2: Using docker run**

```bash
docker run -d \
  --name ddnsd \
  --network host \
  --restart on-failure \
  -e DDNS_IP_SOURCE_TYPE=netlink \
  -e DDNS_IP_SOURCE_INTERFACE=eth0 \
  -e DDNS_PROVIDER_TYPE=cloudflare \
  -e DDNS_PROVIDER_API_TOKEN=your_token \
  -e DDNS_RECORDS=example.com,www.example.com \
  -e DDNS_STATE_STORE_TYPE=memory \
  --memory=64m \
  --cpus=0.5 \
  ddnsd:latest
```

**Option 3: Using docker-compose**

```bash
# Copy and edit docker-compose.yml with your values
cp docker-compose.yml docker-compose.override.yml
nano docker-compose.override.yml

# Run
docker-compose up -d

# View logs
docker-compose logs -f

# Stop
docker-compose down
```

**Management**:

```bash
# View logs
docker logs -f ddnsd

# Check status
docker ps -f name=ddnsd

# Restart
docker restart ddnsd

# Stop
docker stop ddnsd

# Remove
docker rm -f ddnsd
```

---

### 3. Kubernetes

**Best for**: Cloud-native environments, orchestration, multi-cluster

**Advantages**:
- Declarative configuration
- Self-healing
- Rolling updates
- Secrets management

**Installation**:

See `deploy/k8s/README.md` for detailed instructions.

**Quick Deploy**:

```bash
# Deploy using script
./deploy/k8s-deploy.sh
```

**Manual Deploy**:

```bash
# Create namespace
kubectl apply -f deploy/k8s/namespace.yaml

# Create secret
kubectl create secret generic ddnsd-secrets \
  --from-literal=api-token=your_token \
  --namespace=ddns-system

# Create configmap (edit first)
nano deploy/k8s/configmap.yaml
kubectl apply -f deploy/k8s/configmap.yaml

# Deploy
kubectl apply -f deploy/k8s/serviceaccount.yaml
kubectl apply -f deploy/k8s/deployment.yaml
```

**Management**:

```bash
# Check status
kubectl get pods -n ddns-system

# View logs
kubectl logs -f deployment/ddnsd -n ddns-system

# Restart
kubectl rollout restart deployment/ddnsd -n ddns-system

# Update configuration
kubectl edit configmap ddnsd-config -n ddns-system
kubectl rollout restart deployment/ddnsd -n ddns-system

# Scale (NOT recommended - see notes below)
kubectl scale deployment/ddnsd --replicas=1 -n ddns-system
```

**Important**: Do not scale beyond 1 replica. Multiple replicas will make duplicate DNS updates and waste API quota.

---

## Configuration

All configuration is done via environment variables. See `docs/OPS.md` for complete reference.

### Required Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `DDNS_PROVIDER_API_TOKEN` | Provider API token | Cloudflare API token |
| `DDNS_RECORDS` | DNS records to update | `example.com,www.example.com` |
| `DDNS_STATE_STORE_PATH` | State file path (file store) | `/var/lib/ddns/state.json` |

### Common Configuration Patterns

#### Home Server (Linux, systemd)

```bash
DDNS_IP_SOURCE_TYPE=netlink
DDNS_IP_SOURCE_INTERFACE=eth0
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=home.example.com
DDNS_STATE_STORE_TYPE=file
DDNS_STATE_STORE_PATH=/var/lib/ddns/state.json
```

#### Cloud VPS (Docker)

```bash
DDNS_IP_SOURCE_TYPE=http
DDNS_IP_SOURCE_URL=https://api.ipify.org
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=your_token
DDNS_RECORDS=vps.example.com
DDNS_STATE_STORE_TYPE=memory
```

#### Kubernetes Cluster

```bash
DDNS_IP_SOURCE_TYPE=netlink
DDNS_IP_SOURCE_INTERFACE=eth0
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=from_secret
DDNS_RECORDS=cluster.example.com
DDNS_STATE_STORE_TYPE=memory
```

---

## Security Considerations

### Secrets Management

**Never commit API tokens to version control.**

**systemd**:
- File permissions: `/etc/default/ddnsd` should be `640` (root:ddns)
- Environment file is readable by root and ddns user only

**Docker**:
- Pass tokens as environment variables at runtime
- Don't include in Dockerfile
- Consider using Docker secrets or swarm configs

**Kubernetes**:
- Use Kubernetes secrets (base64 encoded)
- Consider External Secrets Operator or Sealed Secrets
- Enable RBAC and restrict secret access

### Resource Limits

Set appropriate resource limits to prevent resource exhaustion:

**systemd**:
```ini
[Service]
MemoryMax=64M
CPUQuota=200%
```

**Docker**:
```bash
--memory=64m --cpus=0.5
```

**Kubernetes**:
```yaml
resources:
  requests:
    memory: "32Mi"
    cpu: "100m"
  limits:
    memory: "64Mi"
    cpu: "200m"
```

### Network Security

The daemon only makes outbound HTTPS connections to:
- DNS provider APIs (e.g., `api.cloudflare.com`)
- IP detection services (if using HTTP IP source)

No inbound ports are opened.

**Firewall rules** (if needed):
```bash
# Allow outbound HTTPS
iptables -A OUTPUT -p tcp --dport 443 -j ACCEPT

# Block all other outbound (if desired)
iptables -P OUTPUT DROP
```

### Process Isolation

**systemd**:
- Runs as non-root user (`ddns`)
- `ProtectSystem=strict`, `ProtectHome=true`
- `ReadWritePaths=/var/lib/ddns` only

**Docker**:
- Runs as non-root user (UID 1000)
- Read-only root filesystem
- `no-new-privileges` security option

**Kubernetes**:
- `runAsNonRoot: true`
- `readOnlyRootFilesystem: true`
- `allowPrivilegeEscalation: false`
- `automountServiceAccountToken: false`

---

## Monitoring & Troubleshooting

### Logs

**systemd**:
```bash
sudo journalctl -u ddnsd -f
```

**Docker**:
```bash
docker logs -f ddnsd
```

**Kubernetes**:
```bash
kubectl logs -f deployment/ddnsd -n ddns-system
```

### Health Checks

The daemon doesn't expose HTTP endpoints. Health is determined by process presence.

**Docker**:
- `HEALTHCHECK` instruction uses `pgrep ddnsd`

**Kubernetes**:
- `livenessProbe`: Checks if process is running
- `readinessProbe`: Checks if process is ready
- `startupProbe`: Gives container time to start

### Common Issues

#### 1. Exit Code 1 (Configuration Error)

**Symptom**: Daemon exits immediately after start

**Diagnosis**:
```bash
# Check logs for "Configuration error:"
journalctl -u ddnsd -n 50
```

**Common causes**:
- Missing `DDNS_PROVIDER_API_TOKEN`
- Empty `DDNS_RECORDS`
- Missing `DDNS_STATE_STORE_PATH` (file store)

**Fix**: Correct configuration, then restart.

#### 2. Exit Code 2 (Runtime Error)

**Symptom**: Daemon starts but crashes or restarts repeatedly

**Diagnosis**:
```bash
# Check logs for "Daemon error:"
journalctl -u ddnsd -n 100
```

**Common causes**:
- Tokio runtime failure (resource limits)
- State file permission error
- Shutdown timeout exceeded

**Fix**: Investigate logs, fix underlying issue, restart.

#### 3. No DNS Updates

**Symptom**: Daemon runs but DNS records not updated

**Diagnosis**:
```bash
# Check for IP change events
journalctl -u ddnsd | grep "IP changed"

# Check for API errors
journalctl -u ddnsd | grep -i error
```

**Common causes**:
- IP hasn't changed (daemon working correctly)
- API token invalid or insufficient permissions
- Rate limiting (too frequent updates)

**Fix**: Verify token and permissions, check rate limits.

#### 4. Container/Pod Restart Loop

**Symptom**: Container or pod keeps restarting

**Diagnosis**:
```bash
# Docker
docker logs ddnsd
docker inspect ddnsd

# Kubernetes
kubectl describe pod -l app=ddnsd -n ddns-system
kubectl logs -l app=ddnsd -n ddns-system --previous
```

**Common causes**:
- Configuration error (exit code 1)
- Resource limits too low (OOMKilled)
- Health check failing

**Fix**: Check logs, increase resources if needed.

### Monitoring Checklist

- [ ] Daemon process is running
- [ ] No exit code 1 errors (config errors)
- [ ] No frequent exit code 2 errors (runtime errors)
- [ ] Logs show successful DNS updates when IP changes
- [ ] No API rate limit errors
- [ ] Resource usage within limits
- [ ] State file is being updated (file store)

---

## Upgrading

### systemd

```bash
# Build new version
cargo build --release

# Stop daemon
sudo systemctl stop ddnsd

# Backup state (optional)
sudo cp /var/lib/ddns/state.json /var/lib/ddns/state.json.backup

# Replace binary
sudo cp target/release/ddnsd /usr/local/bin/

# Start daemon
sudo systemctl start ddnsd

# Verify
sudo systemctl status ddnsd
sudo journalctl -u ddnsd -n 50
```

### Docker

```bash
# Build new image
docker build -t ddnsd:v1.0.1 .

# Stop and remove old container
docker stop ddnsd
docker rm ddnsd

# Run new container (same as before)
DDNS_PROVIDER_API_TOKEN=your_token \
DDNS_RECORDS=example.com \
./deploy/docker-run.sh

# Or with docker-compose
docker-compose down
docker-compose pull  # or: docker build -t ddnsd:latest .
docker-compose up -d
```

### Kubernetes

```bash
# Build and push new image
docker build -t ddnsd:v1.0.1 .
# docker tag ddnsd:v1.0.1 registry.example.com/ddnsd:v1.0.1
# docker push registry.example.com/ddnsd:v1.0.1

# Update deployment image
kubectl set image deployment/ddnsd \
  ddnsd=ddnsd:v1.0.1 \
  -n ddns-system

# Watch rollout
kubectl rollout status deployment/ddnsd -n ddns-system

# Rollback if needed
kubectl rollout undo deployment/ddnsd -n ddns-system
```

---

## Next Steps

After deployment:

1. **Verify**: Check logs to confirm daemon is running and monitoring IP changes
2. **Test**: Trigger an IP change (if possible) and verify DNS update
3. **Monitor**: Set up log aggregation and monitoring
4. **Document**: Record your configuration and procedures
5. **Plan**: Plan for upgrades and disaster recovery

For more information:
- **Operations**: See `docs/OPS.md`
- **Architecture**: See `docs/architecture/ARCHITECTURE.md`
- **Issues**: Report bugs at https://github.com/ddns-lab/ddns/issues
