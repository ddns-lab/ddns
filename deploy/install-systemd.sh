#!/bin/bash
#
# ddnsd systemd installation script
#
# This script installs the ddns daemon as a systemd service.
# It creates the necessary user, directories, and installs service files.
#
# Usage:
#   sudo ./install-systemd.sh
#
# Uninstall:
#   sudo ./install-systemd.sh --uninstall

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Installation paths
BINARY_PATH="/usr/local/bin/ddnsd"
SERVICE_FILE="/etc/systemd/system/ddnsd.service"
ENV_FILE="/etc/default/ddnsd"
STATE_DIR="/var/lib/ddns"
LOG_DIR="/var/log/ddns"
USER="ddns"
GROUP="ddns"

# Uninstall function
uninstall() {
    echo -e "${YELLOW}Uninstalling ddnsd service...${NC}"

    # Stop and disable service
    if systemctl is-active --quiet ddnsd; then
        echo "Stopping ddnsd service..."
        sudo systemctl stop ddnsd
    fi

    if systemctl is-enabled --quiet ddnsd; then
        echo "Disabling ddnsd service..."
        sudo systemctl disable ddnsd
    fi

    # Remove files
    echo "Removing service files..."
    sudo rm -f "$SERVICE_FILE"
    sudo rm -f "$ENV_FILE"
    sudo rm -f "$BINARY_PATH"

    # Remove user and group (optional, keeps state directory)
    echo -e "${YELLOW}Note: User '$USER' and state directory '$STATE_DIR' are preserved.${NC}"
    echo "To remove them manually:"
    echo "  sudo userdel $USER"
    echo "  sudo rm -rf $STATE_DIR $LOG_DIR"

    # Reload systemd
    sudo systemctl daemon-reload

    echo -e "${GREEN}Uninstall complete.${NC}"
    exit 0
}

# Check for uninstall flag
if [ "$1" = "--uninstall" ]; then
    uninstall
fi

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root (sudo).${NC}"
    exit 1
fi

echo -e "${GREEN}Installing ddnsd service...${NC}"

# Check if binary exists
if [ ! -f "target/release/ddnsd" ]; then
    echo -e "${RED}Error: Binary not found at target/release/ddnsd${NC}"
    echo "Please build the daemon first:"
    echo "  cargo build --release"
    exit 1
fi

# Create user and group
if ! id "$USER" &>/dev/null; then
    echo "Creating user '$USER'..."
    useradd -r -s /bin/false -d "$STATE_DIR" "$USER"
else
    echo "User '$USER' already exists."
fi

# Create directories
echo "Creating directories..."
mkdir -p "$STATE_DIR"
mkdir -p "$LOG_DIR"
mkdir -p "$(dirname "$ENV_FILE")"

# Install binary
echo "Installing binary to $BINARY_PATH..."
cp target/release/ddnsd "$BINARY_PATH"
chmod 755 "$BINARY_PATH"

# Install service file
echo "Installing service file to $SERVICE_FILE..."
cp deploy/ddnsd.service "$SERVICE_FILE"
chmod 644 "$SERVICE_FILE"

# Install environment file
if [ ! -f "$ENV_FILE" ]; then
    echo "Installing environment file to $ENV_FILE..."
    cp deploy/ddnsd.default "$ENV_FILE"
    chmod 640 "$ENV_FILE"
    chown root:"$USER" "$ENV_FILE"
    echo -e "${YELLOW}Note: Edit $ENV_FILE to configure the daemon.${NC}"
else
    echo "Environment file already exists at $ENV_FILE"
    echo "Preserving existing configuration."
fi

# Set permissions on state directory
chown -R "$USER:$GROUP" "$STATE_DIR"
chown -R "$USER:$GROUP" "$LOG_DIR"
chmod 750 "$STATE_DIR"
chmod 750 "$LOG_DIR"

# Reload systemd
echo "Reloading systemd..."
systemctl daemon-reload

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Next steps:"
echo "  1. Edit configuration: sudo nano $ENV_FILE"
echo "  2. Enable service: sudo systemctl enable ddnsd"
echo "  3. Start service: sudo systemctl start ddnsd"
echo "  4. Check status: sudo systemctl status ddnsd"
echo "  5. View logs: sudo journalctl -u ddnsd -f"
echo ""
echo "To uninstall:"
echo "  sudo $0 --uninstall"
