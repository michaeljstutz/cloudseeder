# syntax=docker/dockerfile:1.7

FROM rust:1.95-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1 AS builder
WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked \
    && cp target/release/cloudseeder /usr/local/bin/cloudseeder

FROM debian:bookworm-slim@sha256:0104b334637a5f19aa9c983a91b54c89887c0984081f2068983107a6f6c21eeb AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin cloudseeder

COPY --from=builder /usr/local/bin/cloudseeder /usr/local/bin/cloudseeder

# Runtime WORKDIR doubles as the default templates root: the in-binary default
# `templates_dir = "./templates"` resolves to `/etc/cloudseeder/templates`.
# Mount your templates here:
#   docker run -v "$PWD/templates:/etc/cloudseeder/templates:ro" ...
WORKDIR /etc/cloudseeder

USER cloudseeder
ENV CLOUDSEEDER_ADDR=0.0.0.0:8080
EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:8080/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/cloudseeder"]
