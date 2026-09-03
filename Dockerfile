# syntax=docker/dockerfile:1.7

FROM rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 AS builder
WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked \
    && cp target/release/cloudseeder /usr/local/bin/cloudseeder

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS runtime
# Debian point releases can retire exact package versions; keep the base image
# digest-pinned and let security updates resolve from the current bookworm repo.
# hadolint ignore=DL3008
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
