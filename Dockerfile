# syntax=docker/dockerfile:1

# ---- chef (shared base) ----
# cargo-chef lets us cache a dependency-only compile as its own layer, so a
# source-only change no longer rebuilds every crate — a big win for the
# emulated arm64 build. This base layer is itself cached until the Rust image
# or cargo-chef version changes.
FROM rust:1-slim AS chef
# Build deps for rusqlite's bundled SQLite (C compiler).
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked --version 0.1.71
WORKDIR /src

# ---- plan ----
# Distil the dependency graph into recipe.json. This changes only when the
# manifests change, so the cook step below stays cached across source edits.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- build ----
FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
# Compile dependencies only — cached unless recipe.json (i.e. a manifest) changes.
RUN cargo chef cook --release --recipe-path recipe.json
# Now the source; only our own crate recompiles on a source-only change.
COPY . .
RUN cargo build --release --locked
# Pre-create the data dir so it can be copied in with nonroot ownership.
RUN mkdir -p /seed-data

# ---- runtime ----
# distroless/cc:nonroot — glibc + libstdc++, no shell, runs as uid 65532.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /src/target/release/weekling /usr/local/bin/weekling
# Data dir owned by the nonroot user (a named volume inherits this ownership).
COPY --from=builder --chown=65532:65532 /seed-data /var/lib/weekling

ENV WEEKLING_BIND=0.0.0.0:5232 \
    WEEKLING_DATA_DIR=/var/lib/weekling
VOLUME ["/var/lib/weekling"]
EXPOSE 5232

# Already nonroot via the :nonroot tag; stated explicitly for clarity.
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/weekling"]
