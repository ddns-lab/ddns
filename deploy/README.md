# Systemd Configuration Templates

This directory contains **systemd service configuration templates** for reference.

**Note**: The automated installer (`install.sh`) handles all systemd setup automatically. These files are provided for reference only.

## Files

- **ddnsd.service** - Systemd unit file template
  - Defines service behavior, security hardening, restart policy
  - Installed to: `/etc/systemd/system/ddnsd.service`

- **ddnsd.default** - Environment variable template
  - Contains all configuration options with documentation
  - Installed to: `/etc/ddnsd/ddnsd.env`

## Automated Installation (Recommended)

Use the one-line installer for automatic setup:

```bash
curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
```

This will:
- Install the binary
- Create systemd service file
- Create configuration file
- Enable and start the service

## Manual Installation

If you prefer manual setup, use these templates as reference:

```bash
# Copy service file
sudo cp ddnsd.service /etc/systemd/system/

# Copy and edit environment file
sudo cp ddnsd.default /etc/ddnsd/ddnsd.env
sudo vi /etc/ddnsd/ddnsd.env

# Reload and enable
sudo systemctl daemon-reload
sudo systemctl enable ddnsd
sudo systemctl start ddnsd
```

See [Deployment Guide](../docs/user/deployment.md) for detailed instructions.
