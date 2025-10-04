# Build stage
FROM --platform=$BUILDPLATFORM rust:alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

WORKDIR /app/
COPY ./src/ ./src/
COPY ./crates/ ./crates/
COPY ./.cargo/ ./.cargo/
COPY ./Cargo.toml ./Cargo.lock ./rust-toolchain.toml ./
RUN cargo build --release

# Runtime stage
FROM scratch

COPY --from=builder /app/target/**/mchprs /

VOLUME ["/data/"]
WORKDIR /data/
EXPOSE 25565
ENTRYPOINT ["/mchprs"]
