# Build Rust tracker - use nightly for edition2024 support
FROM rustlang/rust:nightly-slim AS builder

WORKDIR /app

# Install dependencies for Solana SDK compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source and manifests
COPY rust/ ./

# Build for release
RUN cargo build --release

# Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /root/
COPY --from=builder /app/target/release/tracker_rs .

CMD ["./tracker_rs"]
