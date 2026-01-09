#!/bin/bash
#
# ddnsd Docker build and run script
#
# This script builds and runs ddnsd in a Docker container.
#
# Usage:
#   ./docker-run.sh                    # Build and run
#   ./docker-run.sh --build-only       # Build only
#   ./docker-run.sh --stop             # Stop and remove container
#   ./docker-run.sh --logs             # Show logs
#   ./docker-run.sh --shell            # Open shell in container

set -e

# Container configuration
IMAGE_NAME="ddnsd"
CONTAINER_NAME="ddnsd"
STATE_DIR="$PWD/docker-state"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Parse arguments
BUILD_ONLY=false
STOP_ONLY=false
SHOW_LOGS=false
OPEN_SHELL=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --build-only)
            BUILD_ONLY=true
            shift
            ;;
        --stop)
            STOP_ONLY=true
            shift
            ;;
        --logs)
            SHOW_LOGS=true
            shift
            ;;
        --shell)
            OPEN_SHELL=true
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Stop and remove container
if [ "$STOP_ONLY" = true ]; then
    echo -e "${YELLOW}Stopping container...${NC}"
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    echo -e "${GREEN}Container stopped and removed.${NC}"
    exit 0
fi

# Show logs
if [ "$SHOW_LOGS" = true ]; then
    docker logs -f "$CONTAINER_NAME"
    exit 0
fi

# Open shell in container
if [ "$OPEN_SHELL" = true ]; then
    if [ ! "$(docker ps -q -f name=$CONTAINER_NAME)" ]; then
        echo -e "${RED}Container is not running. Start it first:${NC}"
        echo "  $0"
        exit 1
    fi
    docker exec -it "$CONTAINER_NAME" /bin/sh
    exit 0
fi

# Build image
echo -e "${GREEN}Building Docker image...${NC}"
docker build -t "$IMAGE_NAME:latest" .

if [ "$BUILD_ONLY" = true ]; then
    echo -e "${GREEN}Build complete.${NC}"
    exit 0
fi

# Stop existing container if running
if [ "$(docker ps -q -f name=$CONTAINER_NAME)" ]; then
    echo -e "${YELLOW}Stopping existing container...${NC}"
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
fi

# Create state directory
mkdir -p "$STATE_DIR"

# Check for required environment variables
if [ -z "$DDNS_PROVIDER_API_TOKEN" ]; then
    echo -e "${RED}Error: DDNS_PROVIDER_API_TOKEN is not set.${NC}"
    echo "Set it via:"
    echo "  export DDNS_PROVIDER_API_TOKEN=your_token"
    echo ""
    echo "Or provide it inline:"
    echo "  DDNS_PROVIDER_API_TOKEN=your_token $0"
    exit 1
fi

if [ -z "$DDNS_RECORDS" ]; then
    echo -e "${RED}Error: DDNS_RECORDS is not set.${NC}"
    echo "Set it via:"
    echo "  export DDNS_RECORDS=example.com,www.example.com"
    echo ""
    echo "Or provide it inline:"
    echo "  DDNS_RECORDS=example.com $0"
    exit 1
fi

# Run container
echo -e "${GREEN}Starting container...${NC}"
docker run -d \
    --name "$CONTAINER_NAME" \
    --network host \
    --restart on-failure \
    -e DDNS_IP_SOURCE_TYPE="${DDNS_IP_SOURCE_TYPE:-netlink}" \
    -e DDNS_IP_SOURCE_INTERFACE="${DDNS_IP_SOURCE_INTERFACE:-eth0}" \
    -e DDNS_PROVIDER_TYPE="${DDNS_PROVIDER_TYPE:-cloudflare}" \
    -e DDNS_PROVIDER_API_TOKEN="$DDNS_PROVIDER_API_TOKEN" \
    -e DDNS_PROVIDER_ZONE_ID="${DDNS_PROVIDER_ZONE_ID:-}" \
    -e DDNS_RECORDS="$DDNS_RECORDS" \
    -e DDNS_STATE_STORE_TYPE="${DDNS_STATE_STORE_TYPE:-memory}" \
    -e DDNS_LOG_LEVEL="${DDNS_LOG_LEVEL:-info}" \
    -e DDNS_MAX_RETRIES="${DDNS_MAX_RETRIES:-3}" \
    -e DDNS_RETRY_DELAY_SECS="${DDNS_RETRY_DELAY_SECS:-5}" \
    --security-opt no-new-privileges:true \
    --read-only \
    --tmpfs /tmp:size=10M,mode=1777 \
    --tmpfs /run:size=1M,mode=1777 \
    --memory=64m \
    --cpus=0.5 \
    "$IMAGE_NAME:latest"

echo ""
echo -e "${GREEN}Container started!${NC}"
echo ""
echo "Check logs:"
echo "  docker logs -f $CONTAINER_NAME"
echo ""
echo "Check status:"
echo "  docker ps -f name=$CONTAINER_NAME"
echo ""
echo "Stop container:"
echo "  docker stop $CONTAINER_NAME"
echo "  # or"
echo "  $0 --stop"
