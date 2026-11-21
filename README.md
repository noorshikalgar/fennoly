# Fennoly

Fennoly is a self-hosted family drive. It keeps your family's documents in one private place where they are easy to store, find, share and work with, from any phone or PC.

- **100% open source** (AGPL-3.0): no paid tiers, no telemetry, no cloud calls.
- **Fast uploads:** parallel, resumable, chunked transfers (tus protocol).
- **Sharing:** with family members on your own server, plus a shared Family space.
- **Built-in tools:** compress and convert files, with a live size preview.
- **Installable PWA** in English, हिंदी and मराठी, with Light, Dark and Saffron themes.

> Status: early development (MVP 1). Not ready for real data yet.

## Stack

| Part | Technology |
| --- | --- |
| Server | Rust: Axum, Tokio, SQLx, PostgreSQL |
| Web app | SvelteKit + TypeScript, Tailwind CSS, PWA |
| File tools | Ghostscript (PDF), `image` crate (images) |
| Deploy | Docker Compose |

## Repository layout

```
crates/
  fennoly-core     domain types, permission rules, safe names
  fennoly-tus      tus 1.0 upload protocol (parallel concatenation)
  fennoly-tools    compress and convert engines
  fennoly-server   HTTP API, auth, storage, sharing
web/               SvelteKit PWA
deploy/            Docker Compose and Dockerfile
```

## Development

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Licence

[GNU AGPL-3.0](LICENSE)
