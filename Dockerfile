# syntax=docker/dockerfile:1

FROM rust:1.88-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates git xz-utils \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/shu /usr/local/bin/shu
ENTRYPOINT ["shu"]
