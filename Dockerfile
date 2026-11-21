# syntax=docker/dockerfile:1

# The validators are 32-bit Windows executables, so the final image must run on amd64.
ARG VALI_PLATFORM=linux/amd64

FROM --platform=$VALI_PLATFORM lukemathwalker/cargo-chef:latest-rust-1-slim-bookworm AS chef

WORKDIR /build

FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

# Build the dependencies first. This layer only changes when the recipe changes.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

# Build the application itself.
RUN rm -rf src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM --platform=$VALI_PLATFORM debian:bookworm-slim

RUN dpkg --add-architecture i386 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        tini \
        wine \
        wine64 \
        wine32:i386 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 10001 --shell /usr/sbin/nologin vali

WORKDIR /app

COPY --from=builder /build/target/release/vali3-docker /usr/local/bin/vali3-docker
COPY vali3/vali-igc.conf ./vali3/vali-igc.conf
COPY vali3/cgi-bin ./vali3/cgi-bin
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh

RUN chmod 0755 /usr/local/bin/docker-entrypoint.sh \
    && chown -R vali:vali /app

USER vali

ENV WINEPREFIX=/home/vali/.wine \
    WINEDEBUG=-all \
    WINEDLLOVERRIDES="mscoree,mshtml="

# Create the Wine prefix at build time. Without this, the first validation would
# initialize it at runtime and Wine may try to download Mono/Gecko.
RUN wineboot --init \
    && wineserver --wait

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint.sh"]
