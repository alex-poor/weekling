# Security Policy

## Reporting a vulnerability

Please report security issues **privately** by opening a
[GitHub security advisory](https://github.com/alex-poor/weekling/security/advisories/new) —
not a public issue or discussion. We'll aim to acknowledge within a few days and keep you
updated on a fix.

If you're unsure whether something is a security issue, err on the side of reporting it privately.

## Supported versions

Weekling is pre-1.0 and moves fast; security fixes land on `main`. Please run the latest build.

## Good to know

- Weekling is **secure by default**: argon2id app passwords for CalDAV clients, cookie + CSRF
  sessions for the admin UI, per-principal isolation, and optional built-in TLS (Let's Encrypt)
  and OIDC SSO.
- OIDC `id_token` signatures are **verified** with the identity provider's public key (via the
  audited `openidconnect` crate); Weekling holds no RSA private key, so the private-key timing
  side-channel tracked in RUSTSEC-2023-0071 is out of scope here.
- The supply chain is gated in CI with `cargo-deny` and `cargo-audit`.
