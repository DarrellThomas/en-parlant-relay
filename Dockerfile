FROM rust:1-bookworm AS builder

WORKDIR /usr/src/app
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo,from=rust:latest,source=/usr/local/cargo \
    --mount=type=cache,target=target \
    cargo build --release && mv ./target/release/en-parlant-relay ./en-parlant-relay

FROM debian:bookworm-slim

RUN useradd -ms /bin/bash app
USER app
WORKDIR /app

COPY --from=builder /usr/src/app/en-parlant-relay /app/en-parlant-relay

EXPOSE 3210

CMD ["./en-parlant-relay"]
