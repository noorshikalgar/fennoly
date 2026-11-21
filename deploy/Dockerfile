# Fennoly: one image with the server, the built web app and the file tools.
#   docker build -f deploy/Dockerfile -t fennoly .

# ---- web app ----
FROM node:22-bookworm-slim AS web
RUN corepack enable
WORKDIR /src/web
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# ---- server ----
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p fennoly-server

# ---- runtime ----
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates ghostscript tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 1000 --home /data fennoly \
 && mkdir -p /data && chown fennoly /data
COPY --from=server /src/target/release/fennoly /usr/local/bin/fennoly
COPY --from=web /src/web/build /app/web
ENV FENNOLY_DATA_DIR=/data \
    FENNOLY_WEB_DIR=/app/web \
    FENNOLY_BIND=0.0.0.0:8686 \
    RUST_LOG=fennoly=info,tower_http=warn
USER fennoly
VOLUME /data
EXPOSE 8686
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["fennoly"]
