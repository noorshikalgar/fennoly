# Developing Fennoly

## Requirements

- Rust 1.85 or later (`rustup`)
- Node 22+ and pnpm 10+
- Docker (for PostgreSQL)
- Optional: Ghostscript (`gs` / `gswin64c`) for PDF compression

## First run

```bash
cp .env.example .env                                   # then set FENNOLY_DATA_DIR to an absolute path
docker compose -f deploy/docker-compose.dev.yml up -d  # PostgreSQL on 127.0.0.1:54329
cargo run -p fennoly-server                            # API on http://127.0.0.1:8686
pnpm --dir web install
pnpm --dir web dev                                     # web app on http://localhost:5173 (proxies /api)
```

Open http://localhost:5173. The first visit shows the setup wizard, which creates the admin account.

## Single-port mode (how it runs in production)

```bash
pnpm --dir web build
FENNOLY_WEB_DIR=./web/build cargo run -p fennoly-server
```

The Rust server then serves the web app and the API on port 8686.

## Checks

```bash
cargo test --workspace                 # unit tests
cargo clippy --workspace --all-targets # lints
pnpm --dir web check                   # svelte-check / TypeScript
scripts/reset-dev.sh                   # wipe the dev database and ./data
scripts/smoke.sh                       # end-to-end API test (needs a fresh, running server)
```

### Sandbox server (keeps your own data safe)

The sandbox uses its own database (`fennoly_test`), data folder (`data-test/`) and build folder
(`target-test/`), on port 8687:

```bash
docker compose -f deploy/docker-compose.dev.yml exec postgres createdb -U fennoly fennoly_test   # once
FENNOLY_DATABASE_URL=postgres://fennoly:fennoly@localhost:54329/fennoly_test FENNOLY_DATA_DIR=./data-test FENNOLY_BIND=127.0.0.1:8687 CARGO_TARGET_DIR=target-test cargo run -p fennoly-server
scripts/reset-test.sh                       # wipe the sandbox
scripts/smoke.sh http://127.0.0.1:8687      # full API + chunked upload tests
```

### Testing bad networks

`scripts/netsim.py` is a proxy that can throttle, cut and black-hole connections:

```bash
python scripts/netsim.py --listen 8690 --target 127.0.0.1:8687 --control 8691
FENNOLY_API=http://127.0.0.1:8690 pnpm --dir web exec vite dev --port 5176
curl "http://127.0.0.1:8691/rate?kbps=2000"         # 2 Mbit/s uplink
curl "http://127.0.0.1:8691/offline?seconds=180"    # 3-minute outage
curl "http://127.0.0.1:8691/flaky?p=0.2"            # random connection cuts
```

See [UPLOADS.md](UPLOADS.md) for how uploads behave and what was tested.

## Layout

| Path | What lives there |
| --- | --- |
| `crates/fennoly-core` | Name rules, permissions. No I/O. |
| `crates/fennoly-tus` | tus protocol parsing. |
| `crates/fennoly-tools` | Image and PDF tools. |
| `crates/fennoly-server` | Axum server. `migrations/` holds the SQL schema. |
| `web/src/lib` | Svelte components, API client, stores, i18n. |
| `web/src/routes` | Pages. `(app)` holds signed-in pages. |

## Conventions

- Access rules live in SQL (`node_access`, `node_live` in `migrations/0002`) so every list agrees.
- Files on disk mirror the folder tree users see: `data/spaces/users/<name>/...` and `data/spaces/family/...`.
- Every user-facing string goes through `t()`; add it to `en.ts` and translate it in `hi.ts` and `mr.ts`
  (the type check fails if a key is missing).
- Git: work on `feat/*`, `fix/*` or `chore/*` branches and merge into `main` with `--no-ff` once checks pass.
