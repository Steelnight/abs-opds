# --- Stage 1: Builder ---
FROM rust:1.91-alpine as builder

WORKDIR /app

# 1. Install Build Dependencies
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static

# --- Dependency Caching Layer ---
# 2. Copy ONLY the cargo manifests first
COPY Cargo.toml Cargo.lock ./

# 3. Create a dummy main.rs to trick cargo into building dependencies
#    This creates a cached layer for all your crates.
RUN mkdir src && \
    echo "fn main() {println!(\"if you see this, the build broke\");}" > src/main.rs

# 4. Build dependencies 
RUN cargo build --release

# --- Application Build Layer ---
# 5. Remove the dummy source code
RUN rm -rf src

# 6. Copy the actual source code
COPY . .

# 7. Build the actual application
#    We touch main.rs to force a rebuild of your app code, linking against the cached deps.
RUN touch src/main.rs && cargo build --release

# --- Stage 2: Runtime ---
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache libssl3 ca-certificates

# Copy the binary
COPY --from=builder /app/target/release/abs_opds /usr/local/bin/abs_opds
COPY --from=builder /app/languages /languages

CMD ["abs_opds"]

