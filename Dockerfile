FROM rust:1.90-slim AS builder

# Create app directory
WORKDIR /app

# Copy actual source
COPY . .

# Build application
RUN cargo build --release

# ---------- Runtime Stage ----------
FROM debian:bookworm-slim

# Create non-root user
RUN useradd -m appuser

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/amialived /app/amialived

RUN chown appuser:appuser /app/amialived
USER appuser
EXPOSE 3000
CMD ["./amialived"]
