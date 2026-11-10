# Weekling — Design Document

> A small, lightning-fast, self-hosted calendar server with a beautiful management UI.
> Linux-only. Written in Rust. Built for homes, families, and small businesses.

**Status:** Draft v0.1 · **Date:** 2026-11-10 · **Owner:** @robot5x

---

## 1. Vision

Weekling is the calendar server you can `docker run` on a Raspberry Pi in two minutes,
hand to your family, and forget about. It speaks CalDAV well enough for the handful of
clients real people use (Apple Calendar, Thunderbird, DAVx⁵, Evening clients), ships a
management UI that looks like it belongs in 2026, and defaults to secure.

It is explicitly **not** a groupware suite, not a Nextcloud, not an Exchange. It does one
category of thing — shared calendars (and tasks) — and does it fast and cleanly.

### Positioning vs. reference projects

| Project  | Lang / Stack        | Strength we borrow                     | Weakness we fix                                  |
|----------|---------------------|----------------------------------------|--------------------------------------------------|
| Radicale | Python, flat files  | Tiny, minimal deps, trivial to deploy  | No real UI, Python runtime, perf under load      |
| DAViCal  | PHP + PostgreSQL    | Strict standards, sharing/delegation   | Heavy stack, dated UX, awkward setup             |
| Baïkal   | PHP + SQLite        | Admin web panel, broad client testing  | PHP runtime, UI is utilitarian not beautiful     |

**Weekling's bet:** a single statically-linked Rust binary with the deployment
simplicity of Radicale, the standards discipline of DAViCal, the admin convenience of
Baïkal — plus a genuinely polished UI and modern auth none of them have.

---

## 2. Goals & Non-Goals

### Goals
- **Single static binary**, no runtime, no interpreter. `scratch`/`distroless` container.
- **Lightning fast & tiny**: idle RAM target < 25 MB; cold start < 100 ms; serve a
  family's year of events with sub-10 ms reads.
- **CalDAV + plain-HTTP(S) sharing** (read-only `.ics` subscription URLs + webcal).
- **State-of-the-art auth**: passkeys/WebAuthn first-class, argon2id passwords, OIDC SSO.
- **Minimal dependencies**: SQLite embedded, TLS via rustls, no external services required.
- **Beautiful, simple management UI** with a guided first-run setup walkthrough.
- **Great on constrained hardware**: Pi / NAS / TrueNAS / Docker; arm64 + amd64.
- **Pragmatic standards coverage** — enough to satisfy the common clients, not every RFC.

### Non-Goals (v1)
- CardDAV / contacts (design leaves room; not built in v1).
- Email, chat, file sync, full groupware.
- Windows/macOS server builds (Linux-only; clients on any OS are fine).
- Clustering / multi-node HA. Single-node is the product.
- Covering every exotic CalDAV client or every RFC corner.

---

## 3. Architecture

```
                    ┌──────────────────────────────────────────┐
                    │                weekling                   │
                    │             (single binary)               │
   CalDAV clients   │  ┌────────────┐      ┌─────────────────┐  │
 ───────────────────┼─▶│  HTTP/1.1  │─────▶│  DAV engine     │  │
  Apple / DAVx⁵ /   │  │  + h2      │      │  (PROPFIND,     │  │
  Thunderbird / …   │  │  (hyper/   │      │   REPORT, sync) │  │
                    │  │   axum)    │      └───────┬─────────┘  │
   Browser (admin)  │  │  rustls    │      ┌───────▼─────────┐  │
 ───────────────────┼─▶│            │─────▶│  Auth / RBAC    │  │
                    │  └────────────┘      └───────┬─────────┘  │
   webcal / .ics    │  ┌────────────┐      ┌───────▼─────────┐  │
 ───────────────────┼─▶│ Static UI  │      │  Store (SQLite  │  │
                    │  │ (embedded) │      │  + iCal parser) │  │
                    │  └────────────┘      └───────┬─────────┘  │
                    │  ┌────────────┐      ┌───────▼─────────┐  │
                    │  │ WebDAV-Push│◀─────│  Change log /   │  │
                    │  │ (Web Push) │      │  sync-token     │  │
                    │  └────────────┘      └─────────────────┘  │
                    └──────────────────────────────────────────┘
                              data dir: /var/lib/weekling
```

Single async process (Tokio). One SQLite database + a config file. Everything else — UI
assets, migrations — embedded in the binary.

### Request layers
1. **Transport** — hyper + rustls (optional, see §8); or plain HTTP behind a reverse proxy.
2. **Router** — axum. Two surfaces: `/dav/*` (CalDAV) and `/` (management UI + JSON API).
3. **Auth middleware** — resolves principal, enforces RBAC per collection.
4. **DAV engine** — WebDAV/CalDAV verbs over the store.
5. **Store** — SQLite (WAL) with a thin repository layer; iCalendar parsed/validated on write.

---

## 4. Technology Choices

| Concern            | Choice                          | Why                                                        |
|--------------------|---------------------------------|------------------------------------------------------------|
| Async runtime      | `tokio`                         | De-facto standard, broad ecosystem                         |
| HTTP server        | `hyper` + `axum`                | Fast, ergonomic routing, tower middleware                  |
| TLS                | `rustls` (+ `rustls-acme`)      | Pure-Rust, memory-safe, optional Let's Encrypt             |
| XML                | `quick-xml`                     | Fast streaming reader/writer for DAV bodies                |
| iCalendar          | `icalendar` / custom parser     | Parse & validate VEVENT/VTODO; keep raw blob too           |
| Storage            | SQLite via `rusqlite`/`sqlx`    | Zero-admin, embedded, WAL concurrency, battle-tested       |
| Passwords          | `argon2` (argon2id)             | Modern KDF                                                  |
| Passkeys           | `webauthn-rs`                   | WebAuthn/FIDO2 server support                               |
| OIDC               | `openidconnect`                 | SSO against Authentik/Keycloak/Google                      |
| UI build           | SvelteKit (static adapter)      | Small bundles, fast, embedded via `rust-embed`             |
| Config             | TOML + env overrides            | Human-editable, 12-factor friendly                         |
| Logging            | `tracing` + `tracing-subscriber`| Structured logs, levels                                    |

**Dependency discipline:** every added crate is a cost. Prefer the small, well-audited
set above. No ORM heavier than needed; no JS framework beyond the UI bundle.

### Why SQLite over flat files (Radicale-style)
Flat files are simple but make sync-tokens, change logs, sharing ACLs, and concurrent
writes awkward. SQLite gives us transactions, a change log for `sync-collection`, and
single-file backup (`weekling backup` → one `.db` copy) while staying zero-admin. Raw
iCalendar is still stored verbatim in a column so we never lose client round-trip fidelity.

---

## 5. Data Model

Core entities (SQLite tables, simplified):

- **principals** — users and groups. `id`, `kind(user|group)`, `display_name`, `email`.
- **credentials** — one principal → many: `password`(argon2id), `webauthn_credential`,
  `oidc_subject`. App-specific passwords live here too (see §8).
- **collections** — a calendar. `id`, `owner_principal`, `display_name`, `color`,
  `description`, `timezone`, `components(VEVENT|VTODO)`, `ctag`, `sync_token`.
- **resources** — one object (an event/task). `id`, `collection_id`, `uid`, `etag`,
  `ical_raw`(blob), plus **extracted** columns for fast range queries: `dtstart`,
  `dtend`, `summary`, `recurrence_rule`, `status`.
- **shares** — `collection_id`, `grantee_principal`, `access(read|read-write|free-busy)`.
- **changes** — append-only log keyed by `sync_token` for `sync-collection` REPORT.
- **subscriptions** — WebDAV-Push registrations (endpoint, keys, expiry).
- **sessions** — browser/admin sessions (UI only; CalDAV is stateless per-request).

Recurrence: store the RRULE verbatim; expand on the fly for time-range REPORTs with a
bounded horizon and a small per-collection expansion cache. No eager materialization.

---

## 6. CalDAV Scope — Pragmatic Coverage

**Target: satisfy the common clients, not the entire RFC surface.** Priorities:

### Must support (v1)
- WebDAV core: `OPTIONS`, `PROPFIND`, `PROPPATCH`, `GET`, `PUT`, `DELETE`, `MKCOL`,
  `MKCALENDAR`, `REPORT`. (RFC 4918 + RFC 4791)
- Service discovery: `.well-known/caldav`, current-user-principal, calendar-home-set.
- `calendar-query` and `calendar-multiget` REPORTs; time-range filtering.
- ETags + `If-Match`/`If-None-Match` for conflict-safe writes.
- CTag + **`sync-collection` (RFC 6578)** for efficient incremental sync.
- VEVENT and **VTODO** (tasks — real demand from Apple Reminders / Tasks.org).
- VTIMEZONE handling; store server default tz; respect per-event tz.
- Free/busy reports (`free-busy-query`) for availability.

### Should support
- **CalDAV sharing** (draft-pot-caldav-sharing lineage): invite/accept for shared calendars,
  mirroring what Apple Calendar expects. We may expose our own simpler sharing in the UI and
  map it onto the protocol where clients understand it.
- **Scheduling (iTIP/iMIP, RFC 6638)** — *phase 2*: auto-process invites between local
  users (iTIP inbox/outbox). iMIP (email) gated behind optional SMTP config.

### Won't do (v1)
- VJOURNAL, VAVAILABILITY beyond free/busy, managed attachments, CalDAV ACL in full
  (we implement a pragmatic subset, not RFC 3744 in its entirety).
- Exotic REPORT types no mainstream client issues.

### Plain-HTTP sharing
- **Read-only subscription:** each calendar gets an unguessable token URL serving a live
  `.ics` (`https://host/ics/<token>.ics`) for `webcal://` subscription in any client.
- **Public page (optional):** a pretty read-only HTML month view at a shareable link.

---

## 7. Management UI

Simple but beautiful. SvelteKit static bundle embedded in the binary; talks to a small
JSON API (`/api/*`). Dark/light, responsive, keyboard-friendly, accessible (WCAG AA).

Screens:
1. **First-run walkthrough** (see §9).
2. **Dashboard** — calendars at a glance, a live week view, recent changes.
3. **Calendars** — create/rename/recolor, set components (events/tasks), sharing.
4. **People** — users & groups, invite by link, reset creds, app passwords.
5. **Sharing** — per-calendar grants (read / read-write / free-busy), subscription links.
6. **Account** — register passkeys, manage sessions, app-specific passwords, theme.
7. **Settings (admin)** — TLS/ACME, OIDC, SMTP (for iMIP), backups, push, logs.
8. **Connect** — per-user, per-client setup helper: shows the exact CalDAV URL, QR code,
   and copy-paste steps for Apple / DAVx⁵ / Thunderbird. Removes the #1 support headache.

Design language: generous whitespace, one accent color per calendar, a real week grid as
the hero (the name is *Weekling*). Ship an opinionated default theme; no theming rabbit hole.

---

## 8. Security & Auth

**Secure by default. No insecure default ever ships enabled.**

### Authentication (layered, admin picks what to enable)
- **Passkeys / WebAuthn** (via `webauthn-rs`) — primary, passwordless path for the UI.
- **Passwords** — argon2id, zxcvbn-style strength check at set time.
- **OIDC SSO** — delegate to Authentik/Keycloak/Google/etc. for orgs that have an IdP.
- **App-specific passwords** — because most CalDAV clients only do HTTP Basic. Each is a
  random high-entropy token, scoped to one principal, revocable, never reused. Basic auth
  is accepted **only** over TLS (or trusted proxy), and only against app passwords —
  never the primary account password.

### Transport
- Built-in rustls TLS with optional **ACME/Let's Encrypt** auto-cert (`rustls-acme`), or
  "behind reverse proxy" mode trusting `X-Forwarded-*` from a configured proxy only.
- HSTS, secure cookies (`HttpOnly`, `SameSite=Lax`), CSRF tokens on the UI API.

### Authorization
- Per-collection RBAC: owner, and shares granting read / read-write / free-busy.
- Principal isolation: a user can never enumerate or reach another's calendar without a share.
- Admin role for server settings; separate from calendar ownership.

### Hardening
- Rate-limiting + exponential backoff on auth endpoints; generic failure messages.
- Argon2id tuned for Pi-class hardware (configurable cost).
- Constant-time credential comparison; no user-enumeration oracles.
- Strict XML parsing: entity-expansion limits, no external entities (billion-laughs/XXE safe).
- Request size limits on `PUT`/report bodies; reject malformed iCalendar at the door.
- Optional audit log of auth and sharing events.
- Supply chain: `cargo-deny` + `cargo-audit` in CI; pinned deps; reproducible builds.

---

## 9. Setup Walkthrough (first-run)

Zero-config to running; guided to secure. On first start with no config:

1. Binary generates a config, prints a **one-time setup URL with a token** to stdout/logs
   (so even headless NAS users can reach it safely — no default admin password to leak).
2. Browser walkthrough:
   - **Welcome** → pick server name & timezone.
   - **Admin account** → create first admin; *prompt to register a passkey immediately*.
   - **Access** → how will clients reach this? (public domain + ACME / behind proxy /
     LAN-only). Weekling validates reachability and, if ACME chosen, provisions a cert.
   - **First calendar** → create one, pick a color.
   - **Connect a client** → show the Connect screen with URL + QR for their chosen app.
   - **Done** → dashboard.
3. CLI parity for automation: `weekling admin create`, `weekling config set …`,
   env-var config for pure-container workflows (no UI click-through required).

---

## 10. Deployment

- **Container:** multi-arch (amd64 + arm64) image on `distroless`/`scratch`, < 20 MB.
  One volume: `/var/lib/weekling` (db + config + certs). One port (default 5232, nod to
  Radicale). Healthcheck endpoint `/healthz`.
- **Compose / TrueNAS:** ship a reference `docker-compose.yml` and a TrueNAS SCALE app
  manifest; a single `WEEKLING_*` env set brings it up fully configured.
- **Bare metal:** single binary + systemd unit (sandboxed: `DynamicUser`,
  `ProtectSystem=strict`, `NoNewPrivileges`). `apt`/`rpm` later.
- **Resource budget:** idle < 25 MB RAM, < 1% CPU; designed to coexist on a Pi running
  other services.
- **Backups:** `weekling backup`/`restore` = consistent SQLite snapshot (one file).
  Online backup via SQLite backup API; document a cron one-liner.
- **Upgrades:** embedded, forward-only migrations run on start; automatic pre-migration
  db copy for rollback safety.

---

## 11. Emerging Paradigms (deliberately adopted)

These are what set a 2026 calendar server apart from the 2012-era references:

- **WebDAV-Push (Web Push, RFC 8030)** — *flagship differentiator.* Implement the
  [draft-bitfire-webdav-push](https://bitfireat.github.io/webdav-push/draft-bitfire-webdav-push-00.html)
  server role so **DAVx⁵** clients get near-instant, battery-friendly push instead of
  polling. Advertise `webdav-push` in the `DAV:` header; manage subscriptions in the
  store. This is the single biggest UX win for Android users and almost nobody self-hosted
  has it yet.
- **Passkeys / WebAuthn** as the primary admin-UI login — passwordless by default.
- **JMAP for Calendars** — *watch, phase 3.* The
  [IETF draft](https://datatracker.ietf.org/doc/draft-ietf-jmap-calendars/) (draft-26,
  Nov 2025) is the likely successor to CalDAV; Stalwart already ships it. Keep the store
  model protocol-agnostic so a JMAP surface can be added later without a rewrite. Do **not**
  build it in v1 — client support isn't there yet.
- **OIDC SSO** as a first-class auth path (homelab IdPs like Authentik are now common).
- **ACME auto-TLS** built in — no manual cert wrangling on a home server.
- **QR/deep-link client onboarding** — treat "connect my phone" as a product surface, not
  a docs page.

---

## 12. Client Compatibility Targets

We test against the realistic short-list (home/family/small-biz), not the long tail:

- **Apple Calendar & Reminders** (macOS/iOS) — includes their sharing + VTODO expectations.
- **DAVx⁵** (Android) — including WebDAV-Push.
- **Mozilla Thunderbird** (desktop).
- **GNOME Calendar / Evolution** (Linux desktop).
- **Tasks.org** (Android, VTODO).
- **webcal/ICS subscription** in anything (read-only path).

A compatibility matrix lives in `/docs/clients.md` and is part of the release checklist;
each release re-runs a litmus test suite (Caldavzap/litmus + per-client manual smoke).

---

## 13. Roadmap

**M0 — Skeleton:** axum server, SQLite store, config, embedded UI shell, `/healthz`, CI
(fmt/clippy/deny/audit), container build.

**M1 — Core CalDAV (MVP):** principals + password/app-password auth; collections +
resources; PROPFIND/REPORT/PUT/GET/DELETE; calendar-query/multiget; ETag; CTag +
sync-collection; VEVENT. Works in Thunderbird + Apple Calendar. Read-only `.ics` share links.

**M2 — UI + onboarding:** management UI (dashboard, calendars, people, connect), first-run
walkthrough, passkeys, dark/light.

**M3 — Sharing & tasks:** per-calendar shares + free/busy; VTODO; CalDAV sharing invites;
DAVx⁵ verified.

**M4 — Modern edge:** WebDAV-Push; ACME auto-TLS; OIDC SSO.

**M5 — Scheduling:** iTIP between local users; optional iMIP via SMTP.

**Later:** JMAP surface, CardDAV, packaged distro builds.

---

## 14. Open Questions

1. **SQLite vs. embedded Postgres-lite** if a user outgrows single-node — do we ever care,
   or is "SQLite forever, scale by not needing to" the stance? *(Lean: SQLite forever.)*
2. **Sharing model surface** — expose CalDAV-sharing protocol invites, or keep sharing
   UI-only + public tokens and skip the messy protocol drafts in v1?
3. **iMIP** — is email-based scheduling in scope for "small business," or deferrable?
4. ~~**UI framework**~~ — **Resolved (2026-11-10): SvelteKit**, static-adapter bundle
   embedded via `rust-embed`. Accepts a Node build step in CI in exchange for the smallest
   bundle and fastest path to a beautiful UI.
5. **Name of the CalDAV path prefix** and default port — `5232` (Radicale-ish) vs `8080`.

---

## 15. Guiding Principles

- Small binary, small RAM, small dependency tree — measured in CI, regressions fail the build.
- Secure by default; the easy path is the safe path.
- Standards where clients need them; no RFC cosplay for its own sake.
- The UI is a feature, not an afterthought.
- If a Raspberry Pi can't run it happily, it's too big.
