# Build stage
FROM --platform=linux/amd64 rust:latest AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs

RUN cargo build --release

# Remove dummy source and fingerprints to force rebuild with real source
RUN rm -rf src target/release/.fingerprint/finance_atp-*

# Copy source code
COPY src ./src

# Build with real source
RUN cargo build --release

# Runtime stage
FROM --platform=linux/amd64 debian:bookworm-slim

# Install runtime dependencies (OpenSSL, ca-certificates, PostgreSQL client for migrations)
RUN apt-get update && \
    apt-get install -y libssl3 ca-certificates postgresql-client curl && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 appuser

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/finance_atp /usr/local/bin/

# Copy migrations (embedded in image)
COPY migrations/ /app/migrations/

# Copy entrypoint script
COPY docker/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# Switch to non-root user
USER appuser

# Environment setup
ENV HOST=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info
ENV DB_HOST=db
ENV DB_USER=postgres
ENV DB_PASSWORD=password
ENV DB_NAME=finance_atp

# Expose port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Run entrypoint script (handles migrations + app start)
ENTRYPOINT ["/app/entrypoint.sh"]
