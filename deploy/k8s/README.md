# Kubernetes Deployment for ddnsd

This directory contains Kubernetes manifests for deploying the ddns daemon in a Kubernetes cluster.

## Prerequisites

- Kubernetes cluster (v1.19+)
- kubectl configured
- Container image built and available (see: `docker build -t ddnsd:latest .`)

## Quick Start

### 1. Create Namespace

```bash
kubectl apply -f namespace.yaml
```

### 2. Create Secret

Replace `your_token_here` with your actual Cloudflare API token:

```bash
kubectl create secret generic ddnsd-secrets \
  --from-literal=api-token=your_token_here \
  --namespace=ddns-system
```

Or edit `secret.yaml` and apply:

```bash
# Edit secret.yaml with your actual token
nano secret.yaml
kubectl apply -f secret.yaml
```

### 3. Configure ddnsd

Edit `configmap.yaml` to configure the daemon:

```bash
nano configmap.yaml
```

Key settings:
- `DDNS_RECORDS`: Comma-separated list of DNS records to update
- `DDNS_IP_SOURCE_INTERFACE`: Network interface to monitor (e.g., `eth0`)
- `DDNS_LOG_LEVEL`: Log level (`trace`, `debug`, `info`, `warn`, `error`)

Apply the config:

```bash
kubectl apply -f configmap.yaml
```

### 4. Deploy

```bash
kubectl apply -f serviceaccount.yaml
kubectl apply -f deployment.yaml
```

### 5. Verify

Check deployment status:

```bash
kubectl get deployment -n ddns-system
kubectl get pods -n ddns-system
```

View logs:

```bash
kubectl logs -f deployment/ddnsd -n ddns-system
```

## Configuration

### Updating Configuration

To update configuration:

```bash
# Edit configmap
kubectl edit configmap ddnsd-config -n ddns-system

# Restart deployment to apply changes
kubectl rollout restart deployment/ddnsd -n ddns-system
```

### Updating Secrets

```bash
# Edit secret
kubectl edit secret ddnsd-secrets -n ddns-system

# Restart deployment
kubectl rollout restart deployment/ddnsd -n ddns-system
```

## Troubleshooting

### Pod Not Starting

```bash
# Check pod status
kubectl describe pod -l app=ddnsd -n ddns-system

# Check logs
kubectl logs -l app=ddnsd -n ddns-system

# Check events
kubectl get events -n ddns-system --sort-by='.lastTimestamp'
```

### Configuration Errors

Exit code 1 indicates a configuration error. Check logs:

```bash
kubectl logs -l app=ddnsd -n ddns-system
```

Look for messages like:
```
Configuration validation error: DDNS_PROVIDER_API_TOKEN is required.
```

### Runtime Errors

Exit code 2 indicates a runtime error. The pod will restart automatically. Check logs for details.

### Common Issues

**Issue**: Pod keeps restarting with exit code 1
- **Cause**: Configuration error or missing secret
- **Fix**: Verify secret and configmap are created correctly

**Issue**: Cannot detect network interface
- **Cause**: `hostNetwork: true` may not be suitable for your cluster
- **Fix**: Remove `hostNetwork` from deployment.yaml and add `NET_ADMIN` capability if needed

**Issue**: OOMKilled
- **Cause**: Memory limit too low
- **Fix**: Increase memory limit in deployment.yaml

## Scaling

**Important**: Do not scale beyond 1 replica.

The ddns daemon is designed to run as a single instance. Multiple replicas will:
- Make duplicate DNS updates (wasting API quota)
- Potentially conflict with each other
- Not provide additional benefit (IP monitoring is per-node)

If you need high availability:
1. Run one instance per node with a DaemonSet
2. Use a leader election mechanism
3. Assign different records to different instances

## Upgrading

To upgrade to a new version:

```bash
# Build new image
docker build -t ddnsd:v1.0.1 .

# Update deployment image
kubectl set image deployment/ddnsd ddnsd=ddnsd:v1.0.1 -n ddns-system

# Or edit deployment
kubectl edit deployment ddnsd -n ddns-system

# Watch rollout
kubectl rollout status deployment/ddnsd -n ddns-system
```

## Uninstall

```bash
kubectl delete -f deployment.yaml
kubectl delete -f serviceaccount.yaml
kubectl delete -f configmap.yaml
kubectl delete -f secret.yaml
kubectl delete -f namespace.yaml
```

## Security Considerations

1. **Secrets Management**: Use proper secrets management (e.g., Sealed Secrets, External Secrets Operator) instead of plain Kubernetes secrets in production
2. **RBAC**: The deployment uses `automountServiceAccountToken: false` to minimize permissions
3. **Network Policies**: Consider adding NetworkPolicies to restrict network access
4. **Pod Security**: The deployment uses security context for non-root user and read-only filesystem
5. **Resource Limits**: Set appropriate resource limits based on your environment

## Monitoring

The daemon logs to stdout/stderr, which Kubernetes captures. Use your logging aggregation platform (ELK, Loki, etc.) to monitor.

Key log patterns to monitor:
- `"Shutting down daemon"` - Normal shutdown
- `"Configuration error"` - Configuration problem
- `"Daemon error"` - Runtime error
- `"Received shutdown signal"` - Graceful shutdown

## Metrics

Currently, the daemon does not expose Prometheus metrics. Future versions may include:
- Update success/failure count
- Current IP address
- Time since last update
- API call duration
