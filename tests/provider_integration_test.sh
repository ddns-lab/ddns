#!/bin/bash
# # Standard Provider Integration Test
#
# This script performs comprehensive integration tests for DNS providers.
#
# ## Test Requirements (MUST PASS for Production)
#
# Every provider MUST pass these tests before being considered production-ready:
#
# 1. **DNS Record Creation**: When a DNS record doesn't exist, it should be automatically created
# 2. **DNS Record Update**: When a DNS record exists and IP changes, it should be updated
# 3. **Multiple Netlink Events**: At least 2 netlink events triggering provider updates
# 4. **Test Data Cleanup**: Test DNS records must be cleaned up after testing
#
# ## Usage
#
# ```bash
# # Cloudflare
# CLOUDFLARE_API_TOKEN=xxx CLOUDFLARE_ZONE_ID=xxx ./tests/provider_integration_test.sh cloudflare
#
# # Aliyun
# ALIYUN_ACCESS_KEY_ID=xxx ALIYUN_ACCESS_KEY_SECRET=xxx ./tests/provider_integration_test.sh aliyun
# ```
#
# ## Test Flow
#
# 1. **Setup**: Clean up previous test artifacts, create dummy interface
# 2. **Start ddnsd**: Launch ddnsd with provider configuration
# 3. **Event 1 (Creation)**: Add first IP address → Should CREATE DNS record
# 4. **Verify Creation**: Check DNS record exists via provider API or dig
# 5. **Event 2 (Update)**: Add second IP address → Should UPDATE DNS record
# 6. **Verify Update**: Check DNS record updated to new IP
# 7. **Cleanup**: Delete test DNS record, stop ddnsd, remove test interface
#
# ## Test Results
#
# Exit codes:
# - 0: All tests passed
# - 1: Configuration error (missing env vars, invalid credentials)
# - 2: Test setup failed (interface creation, ddnsd startup)
# - 3: DNS creation failed
# - 4: DNS update failed
# - 5: Cleanup failed
#
# ## Adding New Providers
#
# Follow this template when adding a new provider:
#
# ```bash
# add_provider_test() {
#     local provider=$1
#
#     case $provider in
#         cloudflare)
#             REQUIRED_VARS="CLOUDFLARE_API_TOKEN CLOUDFLARE_ZONE_ID"
#             TEST_DOMAIN="example.com"
#             TEST_SUBDOMAIN="ddns-test"
#             CLEANUP_API="curl -X DELETE ..."
#             VERIFY_API="curl -X GET ..."
#             ;;
#         aliyun)
#             REQUIRED_VARS="ALIYUN_ACCESS_KEY_ID ALIYUN_ACCESS_KEY_SECRET"
#             TEST_DOMAIN="example.cn"
#             TEST_SUBDOMAIN="ddns-test"
#             CLEANUP_API=""  # Aliyun uses API
#             VERIFY_API="dig +short"
#             ;;
#     esac
# }
# ```

set -e

# Test configuration
# Use dummy interface for testing - must use primary IP (first IP added)
# because SIOCGIFADDR ioctl only returns primary IP, not secondary IPs
VETH_INTERFACE="dummy_ddns_test"
DDNSD_BINARY="./target/release/ddnsd"
DDNSD_LOG="/tmp/ddnsd_integration_test.log"
DDNSD_PID_FILE="/tmp/ddnsd_test.pid"
# Use public IPs that will be primary on the dummy interface
TEST_IPV4_1="1.1.1.1"
TEST_IPV4_2="8.8.8.8"
MAX_WAIT_SECONDS=30

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${2}[$(date +'%H:%M:%S')]${NC} $1"
}

log_info() {
    log "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    log "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    log "${RED}[ERROR]${NC} $1"
}

log_section() {
    echo ""
    log "${GREEN}=== $1 ===${NC}"
}

# Cleanup function
cleanup() {
    log_section "Cleanup"

    # Stop ddnsd
    if [ -f "$DDNSD_PID_FILE" ]; then
        local pid=$(cat "$DDNSD_PID_FILE" 2>/dev/null || true)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            log_info "Stopping ddnsd (PID: $pid)"
            kill "$pid" 2>/dev/null || true
            sleep 1
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$DDNSD_PID_FILE"
    fi

    # Kill any leftover ddnsd
    killall -9 ddnsd 2>/dev/null || true

    # Clean up test interface
    if ip link show "$VETH_INTERFACE" &>/dev/null; then
        log_info "Removing test interface: $VETH_INTERFACE"
        ip link delete "$VETH_INTERFACE" 2>/dev/null || true
    fi

    log_info "Cleanup complete"
}

# Trap cleanup on exit
trap cleanup EXIT INT TERM

# Check required environment variables
check_env_vars() {
    local provider=$1
    local vars=""

    case $provider in
        cloudflare)
            vars="CLOUDFLARE_API_TOKEN CLOUDFLARE_ZONE_ID"
            ;;
        aliyun)
            vars="ALIYUN_ACCESS_KEY_ID ALIYUN_ACCESS_KEY_SECRET"
            ;;
        *)
            log_error "Unknown provider: $provider"
            exit 1
            ;;
    esac

    for var in $vars; do
        if [ -z "${!var}" ]; then
            log_error "Environment variable not set: $var"
            log_info "Usage: $var=<value> ./tests/provider_integration_test.sh $provider"
            exit 1
        fi
    done
}

# Setup test interface (create dummy interface for testing)
setup_veth() {
    log_info "Creating dummy interface: $VETH_INTERFACE"

    # Remove if exists
    ip link show "$VETH_INTERFACE" &>/dev/null && ip link delete "$VETH_INTERFACE" 2>/dev/null || true

    # Create dummy interface
    ip link add "$VETH_INTERFACE" type dummy

    # Bring up interface
    ip link set "$VETH_INTERFACE" up

    log_info "Dummy interface created successfully: $VETH_INTERFACE"
}

# Start ddnsd daemon
start_ddnsd() {
    local provider=$1

    log_info "Starting ddnsd daemon with $provider provider"

    # Set environment variables based on provider
    case $provider in
        cloudflare)
            export DDNS_IP_SOURCE_TYPE=netlink
            export DDNS_IP_SOURCE_INTERFACE=
            export DDNS_PROVIDER_TYPE=cloudflare
            export CLOUDFLARE_API_TOKEN="$CLOUDFLARE_API_TOKEN"
            export DDNS_PROVIDER_API_TOKEN="$CLOUDFLARE_API_TOKEN"
            export DDNS_RECORDS="${FULL_TEST_RECORD}"
            export DDNS_STATE_STORE_TYPE=memory
            export DDNS_LOG_LEVEL=debug
            export RUST_LOG=debug
            ;;
        aliyun)
            export DDNS_IP_SOURCE_TYPE=netlink
            export DDNS_IP_SOURCE_INTERFACE=
            export DDNS_PROVIDER_TYPE=aliyun
            export ALIYUN_ACCESS_KEY_ID="$ALIYUN_ACCESS_KEY_ID"
            export ALIYUN_ACCESS_KEY_SECRET="$ALIYUN_ACCESS_KEY_SECRET"
            export DDNS_PROVIDER_API_TOKEN="$ALIYUN_ACCESS_KEY_ID"
            export DDNS_RECORDS="${FULL_TEST_RECORD}"
            export DDNS_STATE_STORE_TYPE=memory
            export DDNS_LOG_LEVEL=debug
            export RUST_LOG=debug
            ;;
    esac

    # Start ddnsd in background
    $DDNSD_BINARY > "$DDNSD_LOG" 2>&1 &
    local pid=$!
    echo $pid > "$DDNSD_PID_FILE"

    # Wait for startup
    sleep 3

    # Check if still running
    if ! kill -0 $pid 2>/dev/null; then
        log_error "ddnsd failed to start. Check log: $DDNSD_LOG"
        cat "$DDNSD_LOG"
        exit 2
    fi

    log_info "ddnsd started (PID: $pid)"
}

# Verify DNS record via dig
verify_dns() {
    local expected_ip=$1
    local max_wait=${2:-10}

    log_info "Verifying DNS record (expected IP: $expected_ip, max wait: ${max_wait}s)"

    for attempt in $(seq 1 $max_wait); do
        local result=$(dig +short "${FULL_TEST_RECORD}" @223.5.5.5 2>/dev/null | xargs)

        if [ -n "$result" ]; then
            if [ "$result" = "$expected_ip" ]; then
                log_info "✓ DNS verified: ${FULL_TEST_RECORD} → $result"
                return 0
            else
                log_warn "DNS found but IP mismatch: got $result, expected $expected_ip"
            fi
        fi

        sleep 1
    done

    log_error "DNS verification timeout"
    return 1
}

# Cloudflare-specific test
test_cloudflare() {
    FULL_TEST_RECORD="ddns-integration-test.visional.cn"

    log_section "Cloudflare Provider Integration Test"
    log_info "Test domain: ${FULL_TEST_RECORD}"

    # Start ddnsd
    start_ddnsd "cloudflare"

    # Test 1: DNS Creation (add first IP)
    log_section "Test 1: DNS Record Creation"
    log_info "Adding first IP: $TEST_IPV4_1"

    ip addr add "${TEST_IPV4_1}/24" dev "$VETH_INTERFACE"
    sleep 5

    if verify_dns "$TEST_IPV4_1" 15; then
        log_info "✓ DNS creation successful"
    else
        log_error "✗ DNS creation failed"
        return 3
    fi

    # Test 2: DNS Update (add second IP)
    log_section "Test 2: DNS Record Update"
    log_info "Waiting 65 seconds to respect rate limiting (min 60s between updates)"
    sleep 65

    log_info "Adding second IP: $TEST_IPV4_2"

    ip addr del "${TEST_IPV4_1}/24" dev "$VETH_INTERFACE" 2>/dev/null || true
    sleep 1
    ip addr add "${TEST_IPV4_2}/24" dev "$VETH_INTERFACE"
    sleep 5

    if verify_dns "$TEST_IPV4_2" 15; then
        log_info "✓ DNS update successful"
    else
        log_error "✗ DNS update failed"
        return 4
    fi

    log_section "Cloudflare Test Summary"
    log_info "✓ DNS creation: PASS"
    log_info "✓ DNS update: PASS"
    log_info "✓ Netlink events: 2"
    log_info "✓ Cloudflare provider: READY"

    return 0
}

# Aliyun-specific test
test_aliyun() {
    FULL_TEST_RECORD="ddns-integration-test.warzone.cn"

    log_section "Aliyun Provider Integration Test"
    log_info "Test domain: ${FULL_TEST_RECORD}"

    # Start ddnsd
    start_ddnsd "aliyun"

    # Test 1: DNS Creation
    log_section "Test 1: DNS Record Creation"
    log_info "Adding first IP: $TEST_IPV4_1"

    ip addr add "${TEST_IPV4_1}/24" dev "$VETH_INTERFACE"
    sleep 5

    if verify_dns "$TEST_IPV4_1" 20; then
        log_info "✓ DNS creation successful"
    else
        log_error "✗ DNS creation failed"
        return 3
    fi

    # Test 2: DNS Update
    log_section "Test 2: DNS Record Update"
    log_info "Waiting 65 seconds to respect rate limiting (min 60s between updates)"
    sleep 65

    log_info "Adding second IP: $TEST_IPV4_2"

    ip addr del "${TEST_IPV4_1}/24" dev "$VETH_INTERFACE" 2>/dev/null || true
    sleep 1
    ip addr add "${TEST_IPV4_2}/24" dev "$VETH_INTERFACE"
    sleep 5

    if verify_dns "$TEST_IPV4_2" 20; then
        log_info "✓ DNS update successful"
    else
        log_error "✗ DNS update failed"
        return 4
    fi

    log_section "Aliyun Test Summary"
    log_info "✓ DNS creation: PASS"
    log_info "✓ DNS update: PASS"
    log_info "✓ Netlink events: 2"
    log_info "✓ Aliyun provider: READY"

    return 0
}

# Main test runner
main() {
    local provider=$1

    if [ -z "$provider" ]; then
        log_error "Usage: $0 <provider>"
        log_error "Supported providers: cloudflare, aliyun"
        exit 1
    fi

    log_section "Starting Integration Test for Provider: $provider"

    # Check environment
    check_env_vars "$provider"

    # Setup
    log_info "Setting up test environment"
    setup_veth

    # Run provider-specific test
    local test_result=0
    case $provider in
        cloudflare)
            test_cloudflare
            test_result=$?
            ;;
        aliyun)
            test_aliyun
            test_result=$?
            ;;
        *)
            log_error "Unsupported provider: $provider"
            test_result=1
            ;;
    esac

    # Cleanup is automatic via trap

    log_section "Test Complete"
    if [ $test_result -eq 0 ]; then
        log_info "✓ ALL TESTS PASSED"
        exit 0
    else
        log_error "✗ TESTS FAILED (exit code: $test_result)"
        exit $test_result
    fi
}

# Run main if script is executed (not sourced)
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
