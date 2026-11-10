# Changelog

All notable changes to Weekling are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] — 2026-11-10

The first tagged release. 🏋️

### Added

- **Core CalDAV** — service discovery (`/.well-known/caldav`, current-user-principal,
  calendar-home-set), `PROPFIND`/`PROPPATCH`, `MKCALENDAR`, ETag-safe `PUT`/`GET`/`DELETE`, and
  `calendar-query` / `calendar-multiget` / `sync-collection` REPORTs.
- **Sharing** — per-calendar grants (read / read-write / free-busy); shared calendars appear in
  the grantee's home.
- **Tasks & free/busy** — VTODO task lists and `free-busy-query`.
- **Scheduling (RFC 6638)** — implicit iTIP delivery between local users (REQUEST / REPLY /
  CANCEL), with an Invitations screen to accept / decline.
- **WebDAV-Push** — advertisement, subscriptions, and encrypted Web Push delivery
  (RFC 8291 message encryption + RFC 8292 VAPID).
- **Auth & transport** — argon2id app passwords over Basic, cookie + CSRF admin-UI sessions,
  built-in TLS with automatic Let's Encrypt (ACME) or static certs, and optional OIDC SSO.
- **Admin UI** — a SvelteKit "sticker sheet" for calendars, sharing, app passwords, invitations,
  and client setup.
- **Ops** — admin CLI, single static binary, SQLite storage, multi-arch distroless image that
  runs non-root on a read-only root filesystem, and a hardened systemd unit.

[Unreleased]: https://github.com/alex-poor/weekling/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/alex-poor/weekling/releases/tag/v0.1.0
