#!/bin/bash
#
# ddnsd Installer v0.1.0
# Supports: systemd (v0.1.0), docker/docker-compose/k8s (v0.2.0)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/install.sh | sh -s - --mode systemd
#
# Environment variables:
#   DDNS_MODE       Installation mode (auto/systemd/docker) [default: auto]
#   DDNS_VERSION   Version to install [default: latest]
#   DDNS_BINDIR    Installation directory [default: /usr/local/bin]
#   DDNS_CONFIGDIR Config directory [default: /etc/ddnsd]
#   DDNS_NONINTERACTIVE Skip all prompts [default: false]
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO_OWNER="ddns-lab"
REPO_NAME="ddns"
DEFAULT_VERSION="latest"
DEFAULT_BINDIR="/usr/local/bin"
DEFAULT_CONFIGDIR="/etc/ddnsd"
GITHUB_BASE="https://github.com"
GITHUB_API="https://api.github.com"

# Global variables
MODE="${DDNS_MODE:-auto}"
VERSION="${DDNS_VERSION:-${DEFAULT_VERSION}}"
BINDIR="${DDNS_BINDIR:-${DEFAULT_BINDIR}}"
CONFIGDIR="${DDNS_CONFIGDIR:-${DEFAULT_CONFIGDIR}}"
NONINTERACTIVE="${DDNS_NONINTERACTIVE:-false}"
ARCH="$(uname -m)"
OS="$(uname -s)"

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Print header
print_header() {
    cat << "EOF"
 _   _                      ____  _          _ _
| \ | | ___  ___           / ___|| |__   ___| | |
|  \| |/ _ \/ _ \ _____   ____\___ \| '_ \ / _ \ | |
| |\  |  __/ (_) |_____| |_____|__) | |_) |  __/ | |
|_| \_|\___|\___/       |_____|____/|_.__/ \___|_|_|

                   Dynamic DNS Daemon v0.1.0
            https://github.com/ddns-lab/ddns

EOF
}

# Detect system and determine installation mode
detect_mode() {
    log_info "Detecting system..."

    # Check if running in Docker
    if [ -f /.dockerenv ]; then
        log_warn "Detected Docker environment"
        echo "docker"
        return
    fi

    # Check if systemd is available
    if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
        log_info "Detected systemd"
        echo "systemd"
        return
    fi

    # Check if Docker is available
    if command -v docker >/dev/null 2>&1; then
        log_info "Detected Docker"
        echo "docker"
        return
    fi

    log_error "Could not detect supported installation method"
    log_error "Supported methods: systemd, docker"
    return 1
}

# Get latest release version from GitHub
get_latest_version() {
    log_info "Fetching latest version from GitHub..."

    local api_url="${GITHUB_API}/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
    local version

    version=$(curl -fsSL "${api_url}" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "${version}" ]; then
        log_error "Failed to fetch latest version"
        return 1
    fi

    echo "${version}"
}

# Get download URL for the release
get_download_url() {
    local version="$1"
    local os_name
    local arch_name

    # Determine OS name
    case "${OS}" in
        Linux)
            os_name="linux"
            ;;
        Darwin)
            os_name="macos"
            ;;
        *)
            log_error "Unsupported OS: ${OS}"
            return 1
            ;;
    esac

    # Determine architecture
    case "${ARCH}" in
        x86_64|amd64)
            arch_name="x86_64"
            ;;
        aarch64|arm64)
            arch_name="aarch64"
            ;;
        armv7l)
            arch_name="armv7"
            ;;
        *)
            log_error "Unsupported architecture: ${ARCH}"
            return 1
            ;;
    esac

    echo "${GITHUB_BASE}/${REPO_OWNER}/${REPO_NAME}/releases/download/${version}/ddnsd-${version}-${os_name}-${arch_name}.tar.gz"
}

# Download and extract binary
download_binary() {
    local version="$1"
    local download_url
    local tmpdir
    local archive_name

    download_url=$(get_download_url "${version}") || return 1
    tmpdir=$(mktemp -d)
    archive_name="ddnsd-${version}.tar.gz"

    log_info "Downloading from: ${download_url}"

    if ! curl -fsSL -o "${tmpdir}/${archive_name}" "${download_url}"; then
        log_error "Failed to download binary"
        rm -rf "${tmpdir}"
        return 1
    fi

    log_info "Extracting archive..."
    if ! tar -xzf "${tmpdir}/${archive_name}" -C "${tmpdir}"; then
        log_error "Failed to extract archive"
        rm -rf "${tmpdir}"
        return 1
    fi

    # Install binary
    log_info "Installing binary to ${BINDIR}..."
    mkdir -p "${BINDIR}"

    if ! mv "${tmpdir}/ddnsd" "${BINDIR}/ddnsd"; then
        log_error "Failed to install binary (may require sudo)"
        rm -rf "${tmpdir}"
        return 1
    fi

    chmod +x "${BINDIR}/ddnsd"
    rm -rf "${tmpdir}"

    log_success "Binary installed successfully"
}

# Create environment configuration file
create_env_file() {
    local env_file="${CONFIGDIR}/ddnsd.env"

    log_info "Creating environment file: ${env_file}"

    cat > "${env_file}" << 'EOF'
# ddnsd Configuration
# Generated by install.sh

# IP Source Configuration
DDNS_IP_SOURCE_TYPE=netlink
# DDNS_IP_SOURCE_INTERFACE=eth0
# DDNS_IP_SOURCE_URL=https://api.ipify.org
# DDNS_IP_SOURCE_INTERVAL=300

# DNS Provider Configuration
DDNS_PROVIDER_TYPE=cloudflare
DDNS_PROVIDER_API_TOKEN=your_api_token_here
DDNS_PROVIDER_ZONE_ID=your_zone_id_here

# Records to update (comma-separated)
DDNS_RECORDS=example.com,www.example.com

# State Store Configuration
DDNS_STATE_STORE_TYPE=file
DDNS_STATE_STORE_PATH=/var/lib/ddnsd/state.json

# Engine Configuration
# DDNS_MAX_RETRIES=3
# DDNS_RETRY_DELAY_SECS=5
# DDNS_MIN_UPDATE_INTERVAL_SECS=60
# DDNS_LOG_LEVEL=info
EOF

    log_success "Environment file created: ${env_file}"
    log_warn "Please edit ${env_file} and configure your settings"
}

# Create systemd service file
create_systemd_service() {
    local service_file="/etc/systemd/system/ddnsd.service"

    log_info "Creating systemd service: ${service_file}"

    cat > "${service_file}" << EOF
[Unit]
Description=Dynamic DNS Daemon
Documentation=https://github.com/ddns-lab/ddns
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
EnvironmentFile=${CONFIGDIR}/ddnsd.env
ExecStart=${BINDIR}/ddnsd
Restart=always
RestartSec=5s
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${CONFIGDIR} /var/lib/ddnsd

[Install]
WantedBy=multi-user.target
EOF

    # Reload systemd and enable service
    systemctl daemon-reload
    systemctl enable ddnsd.service

    log_success "Systemd service created and enabled"
    log_info "Start with: systemctl start ddnsd"
    log_info "View logs: journalctl -u ddnsd -f"
}

# Prompt for configuration if interactive
prompt_configuration() {
    if [ "${NONINTERACTIVE}" = "true" ]; then
        log_info "Non-interactive mode, using defaults"
        return
    fi

    echo ""
    log_warn "Configuration required!"
    echo ""
    echo "Please provide the following information:"
    echo ""

    # Prompt for Cloudflare API token
    read -p "Cloudflare API Token: " -r api_token
    if [ -n "${api_token}" ]; then
        sed -i "s/your_api_token_here/${api_token}/" "${CONFIGDIR}/ddnsd.env"
    fi

    # Prompt for Zone ID
    read -p "Cloudflare Zone ID (optional, press Enter to skip): " -r zone_id
    if [ -n "${zone_id}" ]; then
        sed -i "s/your_zone_id_here/${zone_id}/" "${CONFIGDIR}/ddnsd.env"
    fi

    # Prompt for records
    read -p "DNS records to update (comma-separated, e.g., example.com,www.example.com): " -r records
    if [ -n "${records}" ]; then
        sed -i "s/example.com,www.example.com/${records}/" "${CONFIGDIR}/ddnsd.env"
    fi

    echo ""
    log_success "Configuration saved to ${CONFIGDIR}/ddnsd.env"
}

# Verify installation
verify_installation() {
    log_info "Verifying installation..."

    # Check if binary exists
    if [ ! -x "${BINDIR}/ddnsd" ]; then
        log_error "Binary not found or not executable: ${BINDIR}/ddnsd"
        return 1
    fi

    # Check version
    local version
    version=$("${BINDIR}/ddnsd" --version 2>/dev/null || echo "unknown")
    log_info "ddnsd version: ${version}"

    # Check if systemd service exists
    if [ "${MODE}" = "systemd" ]; then
        if [ -f "/etc/systemd/system/ddnsd.service" ]; then
            log_success "Systemd service installed"
            systemctl status ddnsd.service --no-pager || true
        else
            log_warn "Systemd service file not found"
        fi
    fi

    log_success "Installation verified successfully"
    echo ""
    echo "Next steps:"
    echo "  1. Edit configuration: vi ${CONFIGDIR}/ddnsd.env"
    echo "  2. Start service: systemctl start ddnsd"
    echo "  3. Check logs: journalctl -u ddnsd -f"
}

# Systemd installation
install_systemd() {
    log_info "Installing ddnsd with systemd..."

    # Check if running as root
    if [ "$(id -u)" -ne 0 ]; then
        log_error "This installation requires root privileges"
        log_error "Please run with sudo"
        return 1
    fi

    # Create directories
    mkdir -p "${BINDIR}"
    mkdir -p "${CONFIGDIR}"
    mkdir -p /var/lib/ddnsd

    # Download binary
    download_binary "${VERSION}" || return 1

    # Create configuration
    create_env_file || return 1

    # Prompt for configuration if interactive
    prompt_configuration

    # Create systemd service
    create_systemd_service || return 1

    # Verify
    verify_installation || return 1

    log_success "Installation completed successfully!"
}

# Docker installation (placeholder for v0.2.0)
install_docker() {
    log_error "Docker installation is not yet supported"
    log_error "This feature is planned for v0.2.0"
    log_info "Please use systemd mode for now"
    return 1
}

# Main installation flow
main() {
    print_header

    # Parse command line arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --mode)
                MODE="$2"
                shift 2
                ;;
            --version)
                VERSION="$2"
                shift 2
                ;;
            --non-interactive)
                NONINTERACTIVE="true"
                shift
                ;;
            --help|-h)
                cat << EOF
Usage: $0 [OPTIONS]

Options:
  --mode <MODE>        Installation mode (auto/systemd/docker) [default: auto]
  --version <VERSION>  Version to install [default: latest]
  --non-interactive    Skip all prompts
  --help, -h           Show this help message

Environment Variables:
  DDNS_MODE           Installation mode
  DDNS_VERSION        Version to install
  DDNS_BINDIR         Installation directory [default: /usr/local/bin]
  DDNS_CONFIGDIR      Config directory [default: /etc/ddnsd]
  DDNS_NONINTERACTIVE Skip prompts

Examples:
  # Auto-detect mode and install latest version
  $0

  # Install specific version with systemd
  $0 --mode systemd --version v0.1.0

  # Non-interactive installation
  DDNS_NONINTERACTIVE=true $0

EOF
                return 0
                ;;
            *)
                log_error "Unknown option: $1"
                return 1
                ;;
        esac
    done

    # Auto-detect mode if needed
    if [ "${MODE}" = "auto" ]; then
        MODE=$(detect_mode) || return 1
    fi

    log_info "Installation mode: ${MODE}"
    log_info "Version: ${VERSION}"

    # Get version if "latest"
    if [ "${VERSION}" = "latest" ]; then
        VERSION=$(get_latest_version) || return 1
        log_info "Latest version: ${VERSION}"
    fi

    # Install based on mode
    case "${MODE}" in
        systemd)
            install_systemd || return 1
            ;;
        docker)
            install_docker || return 1
            ;;
        *)
            log_error "Unsupported mode: ${MODE}"
            return 1
            ;;
    esac
}

# Run main
main "$@"
