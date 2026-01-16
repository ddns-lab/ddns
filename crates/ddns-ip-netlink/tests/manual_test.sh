#!/bin/bash
# Manual Testing Helper for Netlink Integration Tests
#
# This script helps with manual testing of netlink behavior by:
# 1. Creating dummy network interfaces
# 2. Triggering IP changes
# 3. Running the ddnsd daemon with netlink IP source
# 4. Cleaning up test interfaces
#
# Usage:
#   ./manual_test.sh [setup|test|cleanup|watch]

set -e

# Test interface names
TEST_INTERFACE="ddns-test0"
TEST_INTERFACE_2="ddns-test1"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (for network interface manipulation)"
        log_info "Try: sudo $0 $1"
        exit 1
    fi
}

# Create dummy interfaces
setup() {
    log_info "Setting up test interfaces..."

    # Clean up any existing test interfaces
    cleanup

    # Create dummy interfaces
    ip link add ${TEST_INTERFACE} type dummy || log_error "Failed to create ${TEST_INTERFACE}"
    ip link add ${TEST_INTERFACE_2} type dummy || log_error "Failed to create ${TEST_INTERFACE_2}"

    # Bring interfaces up
    ip link set ${TEST_INTERFACE} up
    ip link set ${TEST_INTERFACE_2} up

    # Set initial IPs
    ip addr add 192.168.99.1/24 dev ${TEST_INTERFACE}
    ip addr add 10.0.0.1/24 dev ${TEST_INTERFACE_2}

    log_info "Test interfaces created:"
    ip addr show ${TEST_INTERFACE}
    ip addr show ${TEST_INTERFACE_2}
}

# Trigger IP changes for testing
test() {
    log_info "Triggering IP changes on ${TEST_INTERFACE}..."

    for i in {2..5}; do
        log_info "Setting IP to 192.168.99.${i}"
        ip addr flush dev ${TEST_INTERFACE}
        ip addr add 192.168.99.${i}/24 dev ${TEST_INTERFACE}
        sleep 0.5
    done

    log_info "IP change sequence complete"
    log_info "Check logs for DDNS updates"
}

# Watch for IP changes using ip monitor
watch() {
    log_info "Monitoring IP address changes (Ctrl+C to stop)..."
    log_info "Use another terminal to trigger changes with: $0 test"

    ip -ts monitor addr
}

# Run integration tests
run_tests() {
    log_info "Running netlink integration tests..."

    if ! command -v cargo &> /dev/null; then
        log_error "cargo not found. Please install Rust toolchain."
        exit 1
    fi

    # Run integration tests
    cargo test -p ddns-ip-netlink --test integration_test -- --ignored
}

# Clean up test interfaces
cleanup() {
    log_info "Cleaning up test interfaces..."

    ip link show ${TEST_INTERFACE} &> /dev/null && ip link delete ${TEST_INTERFACE}
    ip link show ${TEST_INTERFACE_2} &> /dev/null && ip link delete ${TEST_INTERFACE_2}

    log_info "Cleanup complete"
}

# Show usage
usage() {
    cat << EOF
Manual Testing Helper for Netlink Integration Tests

Usage: $0 [command]

Commands:
    setup       Create test interfaces (requires root)
    test        Trigger IP changes on test interface (requires root)
    watch       Monitor IP address changes using ip monitor (requires root)
    run_tests   Run cargo integration tests
    cleanup     Delete test interfaces (requires root)
    help        Show this help message

Examples:
    # Setup test interfaces
    sudo $0 setup

    # In another terminal, monitor IP changes
    sudo $0 watch

    # In a third terminal, trigger IP changes
    sudo $0 test

    # Run automated integration tests
    $0 run_tests

    # Clean up when done
    sudo $0 cleanup

Environment Variables:
    TEST_INTERFACE       Test interface name (default: ddns-test0)
    TEST_INTERFACE_2     Second test interface (default: ddns-test1)

EOF
}

# Main
case "${1:-help}" in
    setup)
        check_root setup
        setup
        ;;
    test)
        check_root test
        test
        ;;
    watch)
        check_root watch
        watch
        ;;
    run_tests)
        run_tests
        ;;
    cleanup)
        check_root cleanup
        cleanup
        ;;
    help|--help|-h)
        usage
        ;;
    *)
        log_error "Unknown command: $1"
        usage
        exit 1
        ;;
esac
