# syntax=docker/dockerfile:1

# ---- build ----
FROM rust:1-slim AS builder
WORKDIR /src
# Build deps for rusqlite's bundled SQLite (C compiler).
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*
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
