-- Weekling initial schema (M0). Minimal slice of DESIGN.md §5 — enough to
-- prove the migration path; expanded as the DAV engine lands.

CREATE TABLE principals (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('user', 'group')),
    display_name TEXT NOT NULL,
    email        TEXT,
    is_admin     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE collections (
    id           TEXT PRIMARY KEY,
    owner_id     TEXT NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    description  TEXT,
    color        TEXT,
    timezone     TEXT,
    components   TEXT NOT NULL DEFAULT 'VEVENT',
    ctag         TEXT NOT NULL DEFAULT '0',
    sync_token   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_collections_owner ON collections (owner_id);

CREATE TABLE resources (
    id            TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    uid           TEXT NOT NULL,
    etag          TEXT NOT NULL,
    ical_raw      BLOB NOT NULL,
    dtstart       TEXT,
    dtend         TEXT,
    summary       TEXT,
    status        TEXT,
    updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (collection_id, uid)
);

CREATE INDEX idx_resources_collection ON resources (collection_id);
CREATE INDEX idx_resources_range ON resources (collection_id, dtstart, dtend);
