# Build stage
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev protobuf-dev protoc openssl-dev openssl-libs-static perl

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./

# Copy proto files
COPY proto/ proto/

# Create a dummy main.rs to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy the actual source code
COPY src/ src/

# Touch main.rs to invalidate the cache for the actual build
RUN touch src/main.rs && touch src/lib.rs

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache ca-certificates

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/aimesh /app/aimesh

# Expose default ports (QUIC and HTTP metrics)
EXPOSE 4433/udp 9090

# Set environment variables
ENV RUST_LOG=info

# Run the binary
CMD ["/app/aimesh"]
