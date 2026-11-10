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

# ---- runtime ----
# distroless/cc: glibc + libstdc++, no shell, tiny attack surface.
FROM gcr.io/distroless/cc-debian12 AS runtime
COPY --from=builder /src/target/release/weekling /usr/local/bin/weekling

ENV WEEKLING_BIND=0.0.0.0:5232 \
    WEEKLING_DATA_DIR=/var/lib/weekling
VOLUME ["/var/lib/weekling"]
EXPOSE 5232

ENTRYPOINT ["/usr/local/bin/weekling"]
