#!/bin/bash
#
# ddnsd Uninstaller
#
# Completely removes ddnsd from the system including:
# - Binary
# - Systemd service
# - Configuration files (optional, with confirmation)
# - State data (optional, with confirmation)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ddns-lab/ddns/main/uninstall.sh | sh
#   bash uninstall.sh
#
# Environment variables:
#   DDNS_BINDIR      Binary directory [default: /usr/local/bin]
#   DDNS_CONFIGDIR   Config directory [default: /etc/ddnsd]
#   DDNS_PURGE_ALL   Remove ALL files including config and state [default: false]
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
DEFAULT_BINDIR="/usr/local/bin"
DEFAULT_CONFIGDIR="/etc/ddnsd"

# Global variables
BINDIR="${DDNS_BINDIR:-${DEFAULT_BINDIR}}"
CONFIGDIR="${DDNS_CONFIGDIR:-${DEFAULT_CONFIGDIR}}"
PURGE_ALL="${DDNS_PURGE_ALL:-false}"
NONINTERACTIVE="${DDNS_NONINTERACTIVE:-false}"

# Logging functions
log_info() {
    printf '%b' "${BLUE}[INFO]${NC} $1\n" >&2
}

log_success() {
    printf '%b' "${GREEN}[SUCCESS]${NC} $1\n" >&2
}

log_warn() {
    printf '%b' "${YELLOW}[WARN]${NC} $1\n" >&2
}

log_error() {
    printf '%b' "${RED}[ERROR]${NC} $1\n" >&2
}

# Print header
print_header() {
    printf '\n'
    printf '%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%\n'
    printf '                                                              \n'
    printf '      DDNS Daemon - Uninstall Script                          \n'
    printf '                                                              \n'
    printf '      This will COMPLETELY remove ddnsd from your system      \n'
    printf '                                                              \n'
    printf '%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%%\n'
    printf '\n'
}

# Check if running as root
check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        log_error "This script requires root privileges"
        log_error "Please run with sudo"
        exit 1
    fi
}

# Stop and disable service
stop_service() {
    log_info "Stopping ddnsd service..."

    # Check if service is running
    if systemctl is-active --quiet ddnsd.service 2>/dev/null; then
        systemctl stop ddnsd.service
        log_success "Service stopped"
    else
        log_info "Service was not running"
    fi

    # Disable service
    if systemctl is-enabled --quiet ddnsd.service 2>/dev/null; then
        systemctl disable ddnsd.service
        log_success "Service disabled"
    else
        log_info "Service was not enabled"
    fi
}

# Remove systemd service
remove_service() {
    log_info "Removing systemd service..."

    local service_file="/etc/systemd/system/ddnsd.service"

    if [ -f "$service_file" ]; then
        rm -f "$service_file"
        log_success "Service file removed: $service_file"

        # Reload systemd
        systemctl daemon-reload
        log_success "Systemd reloaded"
    else
        log_info "Service file not found: $service_file"
    fi
}

# Remove binary
remove_binary() {
    log_info "Removing binary..."

    local binary="${BINDIR}/ddnsd"

    if [ -f "$binary" ]; then
        rm -f "$binary"
        log_success "Binary removed: $binary"
    else
        log_info "Binary not found: $binary"
    fi
}

# Remove state directory
remove_state() {
    local state_dir="/var/lib/ddnsd"

    if [ ! -d "$state_dir" ]; then
        log_info "State directory not found: $state_dir"
        return
    fi

    if [ "$PURGE_ALL" = "true" ]; then
        log_info "Removing state directory: $state_dir"
        rm -rf "$state_dir"
        log_success "State directory removed"
        return
    fi

    if [ "$NONINTERACTIVE" = "true" ]; then
        log_info "Preserving state directory: $state_dir"
        log_info "To remove it later, run: DDNS_PURGE_ALL=true $0"
        return
    fi

    # Ask user
    printf '\n'
    log_warn "State directory found: $state_dir"
    printf "This contains DNS update history and state data.\n"
    printf '%s' "Remove state directory? [y/N] "
    read -r response

    case "$response" in
        [yY][eE][sS]|[yY])
            rm -rf "$state_dir"
            log_success "State directory removed"
            ;;
        *)
            log_info "State directory preserved: $state_dir"
            ;;
    esac
}

# Remove configuration
remove_config() {
    if [ ! -d "$CONFIGDIR" ]; then
        log_info "Configuration directory not found: $CONFIGDIR"
        return
    fi

    if [ "$PURGE_ALL" = "true" ]; then
        log_info "Removing configuration directory: $CONFIGDIR"
        rm -rf "$CONFIGDIR"
        log_success "Configuration directory removed"
        return
    fi

    if [ "$NONINTERACTIVE" = "true" ]; then
        log_info "Preserving configuration directory: $CONFIGDIR"
        log_info "To remove it later, run: DDNS_PURGE_ALL=true $0"
        return
    fi

    # Ask user
    printf '\n'
    log_warn "Configuration directory found: $CONFIGDIR"
    printf "This contains your API tokens and DNS provider settings.\n"
    printf '%s' "Remove configuration directory? [y/N] "
    read -r response

    case "$response" in
        [yY][eE][sS]|[yY])
            rm -rf "$CONFIGDIR"
            log_success "Configuration directory removed"
            ;;
        *)
            log_info "Configuration directory preserved: $CONFIGDIR"
            ;;
    esac
}

# Show summary
show_summary() {
    printf '\n'
    printf '==============================================================\n'
    printf '                     Uninstall Summary                       \n'
    printf '==============================================================\n'
    printf '\n'

    local removed_items=""
    local preserved_items=""

    # Check what was removed
    if [ ! -f "${BINDIR}/ddnsd" ]; then
        removed_items="${removed_items}  - Binary: ${BINDIR}/ddnsd\n"
    fi

    if [ ! -f "/etc/systemd/system/ddnsd.service" ]; then
        removed_items="${removed_items}  - Systemd service\n"
    fi

    if [ ! -d "$CONFIGDIR" ]; then
        removed_items="${removed_items}  - Configuration: $CONFIGDIR\n"
    else
        preserved_items="${preserved_items}  - Configuration: $CONFIGDIR\n"
    fi

    if [ ! -d "/var/lib/ddnsd" ]; then
        removed_items="${removed_items}  - State data: /var/lib/ddnsd\n"
    else
        preserved_items="${preserved_items}  - State data: /var/lib/ddnsd\n"
    fi

    if [ -n "$removed_items" ]; then
        printf "${GREEN}Removed:${NC}\n"
        printf "$removed_items"
    fi

    if [ -n "$preserved_items" ]; then
        printf '\n'
        printf "${YELLOW}Preserved:${NC}\n"
        printf "$preserved_items"
        printf '\n'
        log_info "To remove preserved items later, run:"
        printf "  DDNS_PURGE_ALL=true bash $0\n"
    fi

    printf '\n'
    log_success "Uninstall completed!"
    printf '\n'
}

# Main uninstall flow
main() {
    # Parse command line arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --purge-all)
                PURGE_ALL="true"
                shift
                ;;
            --non-interactive)
                NONINTERACTIVE="true"
                shift
                ;;
            --help|-h)
                cat << EOF
Usage: $0 [OPTIONS]

Options:
  --purge-all          Remove ALL files including config and state
  --non-interactive    Skip all prompts (preserve config and state)
  --help, -h           Show this help message

Environment Variables:
  DDNS_BINDIR         Binary directory [default: /usr/local/bin]
  DDNS_CONFIGDIR      Config directory [default: /etc/ddnsd]
  DDNS_PURGE_ALL      Remove all files [default: false]
  DDNS_NONINTERACTIVE Skip prompts [default: false]

Examples:
  # Uninstall (preserve config and state)
  $0

  # Uninstall and remove EVERYTHING
  $0 --purge-all

  # Non-interactive uninstall (preserve config and state)
  $0 --non-interactive

EOF
                return 0
                ;;
            *)
                log_error "Unknown option: $1"
                return 1
                ;;
        esac
    done

    print_header

    # Check for root
    check_root

    # Confirmation
    if [ "$NONINTERACTIVE" != "true" ]; then
        printf '\n'
        log_warn "This will uninstall ddnsd from your system"
        printf '\n'
        printf '%s' "Continue? [y/N] "
        read -r response

        case "$response" in
            [yY][eE][sS]|[yY])
                log_info "Proceeding with uninstall..."
                ;;
            *)
                log_info "Uninstall cancelled by user"
                exit 0
                ;;
        esac
    fi

    # Perform uninstallation
    stop_service
    remove_service
    remove_binary
    remove_config
    remove_state

    # Show summary
    show_summary
}

# Run main
main "$@"
