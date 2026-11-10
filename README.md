# Weekling

A small, lightning-fast, self-hosted calendar server with a beautiful management UI.
Linux-only, written in Rust, built for homes, families, and small businesses.

> **Status: M0 skeleton.** The server boots, runs SQLite migrations, serves `/healthz`
> and an embedded UI placeholder. CalDAV lands at M1. See [`DESIGN.md`](DESIGN.md) for the
> full design and roadmap.

## Quick start

### From source
```fish
cargo run
# then open http://localhost:5232  (probe: http://localhost:5232/healthz)
```

### Docker
```fish
docker compose up --build
```

## Configuration

Defaults work out of the box. Override via `weekling.toml` (see
[`weekling.example.toml`](weekling.example.toml)) or `WEEKLING_*` env vars (env wins):

| Setting          | Env var                   | Default            |
|------------------|---------------------------|--------------------|
| Listen address   | `WEEKLING_BIND`           | `0.0.0.0:5232`     |
| Data directory   | `WEEKLING_DATA_DIR`       | `./data`           |
| Server name      | `WEEKLING_SERVER_NAME`    | `Weekling`         |
| Max body bytes   | `WEEKLING_MAX_BODY_BYTES` | `10485760` (10 MiB)|
| Log filter       | `WEEKLING_LOG`            | `info`             |

## Deployment

- **Docker / Compose / NAS:** [`docker-compose.yml`](docker-compose.yml) — one volume
  (`/var/lib/weekling`), one port (`5232`).
- **systemd:** [`deploy/weekling.service`](deploy/weekling.service) — hardened unit using
  `DynamicUser` + `StateDirectory`.

## Development

```fish
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo deny check   # supply-chain gate
```

## License

AGPL-3.0-or-later *(provisional — see DESIGN.md open questions; confirm before release)*.
