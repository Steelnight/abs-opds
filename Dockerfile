# Builder Stage
FROM rust:1.83-alpine as builder
WORKDIR /app
COPY . .
# Install build dependencies for Alpine (musl)
RUN apk add --no-cache musl-dev
RUN cargo build --release

# Runtime Stage
FROM alpine:3.19
COPY --from=builder /app/target/release/abs_opds /usr/local/bin/abs_opds
COPY --from=builder /app/abs_opds/languages /languages
CMD ["abs_opds"]
