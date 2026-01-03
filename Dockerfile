# Build stage
FROM rust:1.75-slim-bookworm as builder

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy source code
COPY . .
# Touch main.rs to ensure rebuild
RUN touch src/main.rs
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies (OpenSSL, ca-certificates)
RUN apt-get update && \
    apt-get install -y libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/finance_atp /usr/local/bin/

# Environment setup
ENV HOST=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info

# Expose port
EXPOSE 3000

# Run application
ENTRYPOINT ["finance_atp"]
