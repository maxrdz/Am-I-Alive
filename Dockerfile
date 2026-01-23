FROM rust:1.90-slim AS builder

WORKDIR /app

# Build with only dependencies to cache them at this stage of the docker build
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy actual source and build
COPY . .
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
