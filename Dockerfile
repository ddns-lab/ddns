# Multi-stage Dockerfile for ddnsd
#
# Build stage: Uses Rust image to compile the binary
# Runtime stage: Uses minimal Alpine image for production
#
# Build:
#   docker build -t ddnsd:latest .
#
# Run:
#   docker run -d --name ddnsd --network host \
#     -e DDNS_PROVIDER_API_TOKEN=your_token \
#     -e DDNS_RECORDS=example.com \
#     ddnsd:latest

# =============================================================================
# Build Stage
# =============================================================================
# Use Rust 1.91+ for Edition 2024 support
FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    # For netlink support (Linux)
    libnetfilter_queue-dev

WORKDIR /build

# Copy manifests and source
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY examples/ ./examples/

# Build release binary
RUN cargo build --release --bin ddnsd

# =============================================================================
# Runtime Stage
# =============================================================================
FROM alpine:3.19

# Install runtime dependencies (ca-certificates for SSL/TLS)
RUN apk add --no-cache ca-certificates

# Create non-root user
RUN addgroup -S ddns && \
    adduser -S ddns -G ddns -h /var/lib/ddns -s /sbin/nologin

# Create state directory
RUN mkdir -p /var/lib/ddns && \
    chown -R ddns:ddns /var/lib/ddns

# Copy binary from builder
COPY --from=builder /build/target/release/ddnsd /usr/local/bin/ddnsd

# Set permissions
RUN chmod 755 /usr/local/bin/ddnsd

# Switch to non-root user
USER ddns

# Set environment variables
ENV DDNS_STATE_STORE_TYPE=memory \
    DDNS_LOG_LEVEL=info

# Health check
# The daemon doesn't expose HTTP endpoints, so we check if process is running
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD pgrep ddnsd || exit 1

# Expose note: No ports needed (daemon makes outbound connections only)
# But documentation for clarity
# EXPOSE 53/udp  # Not needed - we use provider APIs, not serve DNS

ENTRYPOINT ["ddnsd"]
CMD []
