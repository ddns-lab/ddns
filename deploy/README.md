# Deployment Artifacts

This directory contains deployment artifacts for the ddns daemon across different platforms.

## Directory Structure

```
deploy/
├── README.md                 # This file
├── install-systemd.sh        # systemd installation script
├── docker-run.sh             # Docker build and run script
├── k8s-deploy.sh             # Kubernetes deployment script
├── ddnsd.service             # systemd unit file
├── ddnsd.default             # systemd environment file template
└── k8s/                      # Kubernetes manifests
    ├── namespace.yaml
    ├── secret.yaml
    ├── configmap.yaml
    ├── serviceaccount.yaml
    ├── deployment.yaml
    └── README.md
```

## Quick Reference

### systemd (Linux)

```bash
# Automated installation
sudo ./install-systemd.sh

# Manual configuration
sudo nano /etc/default/ddnsd

# Start service
sudo systemctl start ddnsd

# Check status
sudo systemctl status ddnsd

# View logs
sudo journalctl -u ddnsd -f
```

### Docker

```bash
# Build and run
DDNS_PROVIDER_API_TOKEN=your_token \
DDNS_RECORDS=example.com,www.example.com \
./docker-run.sh

# Or use docker-compose
docker-compose up -d

# View logs
docker logs -f ddnsd
```

### Kubernetes

```bash
# Deploy
./k8s-deploy.sh

# Check status
kubectl get pods -n ddns-system

# View logs
kubectl logs -f deployment/ddnsd -n ddns-system
```

## File Descriptions

### Systemd

**ddnsd.service**
- systemd unit file for the ddns daemon
- Configures user, security hardening, resource limits
- Installed to: `/etc/systemd/system/ddnsd.service`

**ddnsd.default**
- Environment variable template
- Contains all configuration options with documentation
- Installed to: `/etc/default/ddnsd`

**install-systemd.sh**
- Automated installation script
- Creates user, directories, installs files
- Usage: `sudo ./install-systemd.sh`
- Uninstall: `sudo ./install-systemd.sh --uninstall`

### Docker

**Dockerfile** (in parent directory)
- Multi-stage Docker build
- Minimal runtime image (Alpine-based)
- Health checks included

**docker-run.sh**
- Docker build and run script
- Validates required environment variables
- Usage: `DDNS_PROVIDER_API_TOKEN=token DDNS_RECORDS=records ./docker-run.sh`

**docker-compose.yml** (in parent directory)
- Docker Compose configuration
- Override with `docker-compose.override.yml` for custom settings

### Kubernetes

**k8s/namespace.yaml**
- Defines `ddns-system` namespace
- Isolated namespace for ddns resources

**k8s/secret.yaml**
- Secret template for sensitive data (API tokens)
- Create manually or use `kubectl create secret`

**k8s/configmap.yaml**
- ConfigMap for non-sensitive configuration
- Customize for your environment

**k8s/serviceaccount.yaml**
- ServiceAccount for the deployment
- Token mounting disabled for security

**k8s/deployment.yaml**
- Deployment manifest
- Includes resource limits, security context, probes

**k8s-deploy.sh**
- Kubernetes deployment script
- Creates namespace, secret, configmap, deployment
- Usage: `./k8s-deploy.sh`
- Undeploy: `./k8s-deploy.sh --undeploy`

## Platform Selection Guide

| Platform | Use Case | Advantages |
|----------|----------|------------|
| **systemd** | Bare metal, VPS, home servers | Native supervision, boot startup, integrated logging |
| **Docker** | Edge devices, testing, container hosts | Isolation, portability, easy rollback |
| **Kubernetes** | Cloud-native, orchestration, multi-cluster | Self-healing, rolling updates, declarative config |

## Security Notes

1. **API Tokens**: Never commit to version control
   - systemd: File permissions 640 (root:ddns)
   - Docker: Runtime environment variable
   - Kubernetes: Use secrets, consider Sealed Secrets

2. **Resource Limits**:
   - Memory: 64MB limit (32MB request)
   - CPU: 200% limit (100% request)

3. **Process Isolation**:
   - Runs as non-root user
   - Read-only root filesystem (Docker/K8s)
   - Security hardening enabled (systemd)

## Common Operations

### Updating Configuration

**systemd**:
```bash
sudo nano /etc/default/ddnsd
sudo systemctl restart ddnsd
```

**Docker**:
```bash
# Stop and remove
docker stop ddnsd && docker rm ddnsd

# Run with new config
DDNS_PROVIDER_API_TOKEN=new_token ./docker-run.sh
```

**Kubernetes**:
```bash
kubectl edit configmap ddnsd-config -n ddns-system
kubectl rollout restart deployment/ddnsd -n ddns-system
```

### Viewing Logs

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

### Upgrading

**systemd**:
```bash
cargo build --release
sudo systemctl stop ddnsd
sudo cp target/release/ddnsd /usr/local/bin/
sudo systemctl start ddnsd
```

**Docker**:
```bash
docker build -t ddnsd:new-version .
docker stop ddnsd && docker rm ddnsd
DDNS_PROVIDER_API_TOKEN=token ./docker-run.sh  # Uses new image
```

**Kubernetes**:
```bash
kubectl set image deployment/ddnsd ddnsd=ddnsd:new-version -n ddns-system
kubectl rollout status deployment/ddnsd -n ddns-system
```

## Troubleshooting

For common issues and solutions, see:
- **Operations Guide**: `../docs/OPS.md`
- **Deployment Guide**: `../docs/DEPLOYMENT.md`
- **Kubernetes Specific**: `k8s/README.md`

## Support

For issues or questions:
1. Check documentation in `../docs/`
2. Review logs for error messages
3. Check GitHub issues: https://github.com/ddns-lab/ddns/issues
