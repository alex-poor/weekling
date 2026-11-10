# Weekling

A small, lightning-fast, self-hosted calendar server with a beautiful management UI.
Linux-only, written in Rust, built for homes, families, and small businesses.

> **Status: M1 — core CalDAV.** The server speaks enough CalDAV to connect and sync
> with mainstream clients: service discovery, PROPFIND, PUT/GET/DELETE with ETags,
> MKCALENDAR/PROPPATCH, and REPORTs (calendar-query with time-range, calendar-multiget,
> sync-collection). Auth is argon2id app passwords over HTTP Basic. The management UI
> is still a placeholder (M2). Since then: calendar sharing + tasks + free-busy (M3),
> and WebDAV-Push advertisement + subscriptions (M4; encrypted delivery in progress).
> See [`DESIGN.md`](DESIGN.md) for the full design and roadmap.

## Set up a calendar (admin CLI)

```fish
weekling user add alice --display "Alice"
echo "a-strong-passphrase" | weekling user passwd alice   # account password (UI login)
weekling token add alice              # prints an app password — copy it once
weekling calendar add alice "Home" --id home --color "#5b8defff"
weekling calendar add alice "Tasks" --id tasks --components VTODO   # a task list
weekling calendar ls alice
weekling share add alice home bob --access read-write              # share with another user
weekling share ls alice home
```

The account password logs in to the management API at `/api` (cookie session +
CSRF); CalDAV clients use the app password. The admin UI that consumes `/api`
lands in M2.

Then point a CalDAV client (Apple Calendar, Thunderbird, DAVx⁵, GNOME Calendar) at the
server URL `http://<host>:5232/` (it auto-discovers via `/.well-known/caldav`), with the
username and the app password. Run `weekling help` for all commands.

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
