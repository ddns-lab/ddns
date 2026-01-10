# Installation Guide

## Quick Install (Systemd)

The easiest way to install ddnsd is using the automated install script:

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

This will:
1. Detect your system and install using systemd
2. Download the latest release from GitHub
3. Install the binary to `/usr/local/bin/ddnsd`
4. Create configuration file at `/etc/ddnsd/ddnsd.env`
5. Set up systemd service

## Installation Modes

### Auto-detect (Recommended)
```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

### Systemd Only
```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh -s - --mode systemd
```

### Specific Version
```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh -s - --version v0.1.0
```

### Non-Interactive
```bash
DDNS_NONINTERACTIVE=true DDNS_MODE=systemd curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

## Post-Installation

### 1. Configure ddnsd

Edit the configuration file:
```bash
vi /etc/ddnsd/ddnsd.env
```

Required settings:
- `DDNS_PROVIDER_API_TOKEN` - Your Cloudflare API token
- `DDNS_RECORDS` - DNS records to update (e.g., `example.com,www.example.com`)

### 2. Start the Service

```bash
systemctl start ddnsd
```

### 3. Enable Auto-Start on Boot

```bash
systemctl enable ddnsd
```

### 4. Check Status

```bash
systemctl status ddnsd
```

### 5. View Logs

```bash
journalctl -u ddnsd -f
```

## Manual Installation

### Download Binary

```bash
# Download latest release
wget https://github.com/ddns-lab/ddns/releases/latest/download/ddnsd-v0.1.0-linux-x86_64.tar.gz

# Extract
tar -xzf ddnsd-v0.1.0-linux-x86_64.tar.gz

# Install
sudo mv ddnsd /usr/local/bin/
sudo chmod +x /usr/local/bin/ddnsd
```

### Create Configuration

```bash
sudo mkdir -p /etc/ddnsd
sudo cp examples/ddnsd.env.example /etc/ddnsd/ddnsd.env
sudo vi /etc/ddnsd/ddnsd.env
```

### Create Systemd Service

```bash
sudo cat > /etc/systemd/system/ddnsd.service << 'EOF'
[Unit]
Description=Dynamic DNS Daemon
Documentation=https://github.com/ddns-lab/ddns
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
EnvironmentFile=/etc/ddnsd/ddnsd.env
ExecStart=/usr/local/bin/ddnsd
Restart=always
RestartSec=5s
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable ddnsd
```

## Uninstallation

### Stop and Disable Service
```bash
sudo systemctl stop ddnsd
sudo systemctl disable ddnsd
```

### Remove Files
```bash
sudo rm /etc/systemd/system/ddnsd.service
sudo rm /usr/local/bin/ddnsd
sudo rm -rf /etc/ddnsd
sudo rm -rf /var/lib/ddnsd
```

### Reload Systemd
```bash
sudo systemctl daemon-reload
```

## Troubleshooting

### Service Not Starting

Check logs:
```bash
journalctl -u ddnsd -n 50
```

### Permission Issues

Ensure the binary is executable:
```bash
sudo chmod +x /usr/local/bin/ddnsd
```

### Configuration Issues

Validate your environment file:
```bash
cat /etc/ddnsd/ddnsd.env
```

### Firewall Issues

Ensure outbound HTTPS is allowed:
```bash
sudo iptables -L OUTPUT -v -n
```

## Next Steps

- Configure your DNS records
- Set up monitoring
- Configure log rotation
