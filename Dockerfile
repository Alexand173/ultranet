# Stage 1: Build
FROM rust:1.75-slim AS builder

# Optimized build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    cmake \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/ultranet
COPY . .

# Compile production binary with the committed dependency lockfile.
# LTO and other optimizations are handled via Cargo.toml.
RUN cargo build --release --locked --bin UltraNet

# Stage 2: Production Runtime
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Run the node without root privileges.
RUN useradd --system --uid 10001 --create-home --home-dir /home/ultranet ultranet \
    && mkdir -p /app/ultranet_db \
    && chown -R ultranet:ultranet /app

# Copy binary and static assets
COPY --from=builder /usr/src/ultranet/target/release/UltraNet /app/ultranet
COPY --from=builder /usr/src/ultranet/public /app/public

ENV ULTRANET_API_BIND=0.0.0.0:8081 \
    ULTRANET_DB_PATH=/app/ultranet_db \
    ULTRANET_CORS_ORIGINS=http://localhost:3000,http://127.0.0.1:3000

# Protocol Ports
# 9000: P2P Mainnet Swarm
# 8081: AI-Governance Dashboard & API
EXPOSE 9000 8081

# Persistence Configuration
VOLUME ["/app/ultranet_db"]

USER ultranet

# Healthcheck to verify REST API responsiveness
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -f http://localhost:8081/api/stats || exit 1

ENTRYPOINT ["/app/ultranet"]
