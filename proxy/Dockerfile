# NexusCode proxy — Dockerfile (multi-stage)
# Build: docker build -t nexuscode-proxy .
# Run:  docker run -e GCP_PROJECT_NUMBER=... -e PORT=8080 -p 8080:8080 nexuscode-proxy
# Fly.io dung file nay qua `fly launch` / `fly deploy` (khong can build local).
FROM rust:1.100-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/nexuscode-proxy /usr/local/bin/nexuscode-proxy
ENV PORT=8080
EXPOSE 8080
CMD ["nexuscode-proxy"]
