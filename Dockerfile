# Builder Stage
FROM rust:1.91-alpine as builder
WORKDIR /app
COPY . .
# Install build dependencies for Alpine (musl)
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
RUN cargo build --release

# Runtime Stage
FROM alpine:3.19
# Install runtime dependencies (OpenSSL)
RUN apk add --no-cache libssl3 ca-certificates
COPY --from=builder /app/target/release/abs_opds /usr/local/bin/abs_opds
COPY --from=builder /app/languages /languages
CMD ["abs_opds"]
