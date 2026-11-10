# Contributing to Weekling

Thanks for being here! 🏋️ Weekling is a small, friendly project and contributions —
bug reports, ideas, docs, or code — are all welcome.

## What Weekling tries to be

Keep these in mind; they shape most decisions:

- **Small & fast.** A single static binary, minimal dependencies, low memory. If a Raspberry
  Pi can't run it happily, it's too big.
- **Secure by default.** The easy path is the safe path.
- **Pragmatic CalDAV.** Enough of the standards to satisfy the clients real people use
  (Apple Calendar, Thunderbird, DAVx⁵, GNOME Calendar) — not every RFC corner.
- **The UI is a feature**, not an afterthought.

See [`DESIGN.md`](DESIGN.md) for the architecture and roadmap.

## Getting set up

You'll need **Rust 1.88+** and, if you touch the UI, **Node 20+**.

```sh
# run the server (serves the pre-built UI from web/)
cargo run
# open http://localhost:5232 · probe http://localhost:5232/healthz

# seed a user to log in / connect a client
cargo run -- user add alice --email alice@example.com
echo "a-strong-passphrase" | cargo run -- user passwd alice
cargo run -- token add alice
cargo run -- calendar add alice "Home" --id home --color "#3b5bff"
```

### Working on the UI

The SvelteKit app lives in [`ui/`](ui/) and **builds into [`web/`](web/)**, which is committed so
that `cargo build` and the Docker image need no Node. When you change anything under `ui/`,
rebuild and commit the result:

```sh
cd ui
npm ci
npm run build          # writes the static bundle to ../web
```

## Before you open a PR

Please make sure these pass — CI runs the same checks:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check       # supply-chain (licenses + advisories)
```

New behaviour deserves a test. We lean on fast unit tests plus end-to-end checks against a
running server (the commit messages show the `curl` walkthroughs used for each feature) —
matching that style for CalDAV correctness is very welcome.

## Commits & branches

- **Conventional Commits:** `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`; subject ≤ 72 chars.
- Branch from `main` as `feat/…`, `fix/…`, or `chore/…`; don't commit directly on `main`.
- Keep a PR scoped to one logical change, and say what you tested.

## Bugs & ideas

- 🐛 **Bugs** → open an [issue](https://github.com/alex-poor/weekling/issues) with steps to
  reproduce, the client/app you're using, and relevant logs (`WEEKLING_LOG=debug`).
- 💡 **Ideas & questions** → start a
  [discussion](https://github.com/alex-poor/weekling/discussions).
- 🔒 **Security issues** → please report privately via a
  [GitHub security advisory](https://github.com/alex-poor/weekling/security/advisories/new)
  rather than a public issue.

## License

By contributing, you agree that your contributions are licensed under the project's
[AGPL-3.0-or-later](LICENSE).
