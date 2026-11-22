# Fennoly: one image with the server, the built web app, the file tools and
# the backup engines (restic, rclone, pg_dump).
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

# ---- backup engines: official releases, pinned and checksum-verified ----
FROM debian:bookworm-slim AS engines
ARG TARGETARCH
ARG RESTIC_VERSION=0.19.1
ARG RCLONE_VERSION=1.76.0
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl bzip2 unzip
WORKDIR /dl
RUN set -eu; \
    arch="${TARGETARCH:-amd64}"; \
    case "$arch" in \
      amd64) restic_sum=f415415624dcc452f2a02b8c33641791a8c6d6d3b65bbb3543fcf9a25151585c; \
             rclone_sum=b00f00b3266c3764413afe599d3ec316f6f43b3e0ea8a0bc1d186d2133e46df7 ;; \
      arm64) restic_sum=a5f64aaab53d51e311fa3829124c5b703f2d14cf187d8640b6be3b2b49376465; \
             rclone_sum=ee169016675e23a69d77240565133ff7dbc9b33d16fa8f7053d4af39f1eaeb78 ;; \
      *) echo "unsupported architecture: $arch"; exit 1 ;; \
    esac; \
    curl -fsSLo restic.bz2 "https://github.com/restic/restic/releases/download/v${RESTIC_VERSION}/restic_${RESTIC_VERSION}_linux_${arch}.bz2"; \
    echo "${restic_sum}  restic.bz2" | sha256sum -c -; \
    bunzip2 restic.bz2; \
    install -m 0755 restic /usr/local/bin/restic; \
    curl -fsSLo rclone.zip "https://downloads.rclone.org/v${RCLONE_VERSION}/rclone-v${RCLONE_VERSION}-linux-${arch}.zip"; \
    echo "${rclone_sum}  rclone.zip" | sha256sum -c -; \
    unzip -q rclone.zip; \
    install -m 0755 "rclone-v${RCLONE_VERSION}-linux-${arch}/rclone" /usr/local/bin/rclone

# ---- runtime ----
FROM debian:bookworm-slim
# pg_dump must be at least as new as the PostgreSQL server (17), which is newer
# than Debian's own, so it comes from the PostgreSQL project's apt repository.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl ghostscript tini tzdata \
 && curl -fsSLo /usr/share/keyrings/pgdg.asc https://www.postgresql.org/media/keys/ACCC4CF8.asc \
 && echo "deb [signed-by=/usr/share/keyrings/pgdg.asc] https://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" \
      > /etc/apt/sources.list.d/pgdg.list \
 && apt-get update \
 && apt-get install -y --no-install-recommends postgresql-client-17 \
 && apt-get purge -y curl && apt-get autoremove -y \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 1000 --home /data fennoly \
 && mkdir -p /data /backups && chown fennoly /data /backups
COPY --from=engines /usr/local/bin/restic /usr/local/bin/rclone /usr/local/bin/
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
