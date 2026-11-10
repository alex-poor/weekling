-- Weekling schema (M1). See DESIGN.md §5.
-- Forward-only: once released, never edit an applied script — add a new one.

CREATE TABLE principals (
    id           TEXT PRIMARY KEY,              -- also the URL/login name
    kind         TEXT NOT NULL CHECK (kind IN ('user', 'group')),
    display_name TEXT NOT NULL,
    email        TEXT,
    is_admin     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Authentication secrets. M1 ships app-specific passwords (argon2id); password /
-- webauthn / oidc rows land in later milestones under new `kind` values.
CREATE TABLE credentials (
    id           TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('app_password')),
    label        TEXT NOT NULL,
    secret_hash  TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);

CREATE INDEX idx_credentials_principal ON credentials (principal_id);

CREATE TABLE collections (
    id           TEXT PRIMARY KEY,
    owner_id     TEXT NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    description  TEXT,
    color        TEXT,
    timezone     TEXT,
    components   TEXT NOT NULL DEFAULT 'VEVENT',
    sync_token   INTEGER NOT NULL DEFAULT 0,       -- monotonic per collection
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_collections_owner ON collections (owner_id);

CREATE TABLE resources (
    id            TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    uid           TEXT NOT NULL,                   -- also the <uid>.ics filename stem
    etag          TEXT NOT NULL,
    ical_raw      TEXT NOT NULL,
    dtstart       TEXT,                            -- normalized UTC 'YYYYMMDDTHHMMSSZ'
    dtend         TEXT,
    summary       TEXT,
    status        TEXT,
    has_rrule     INTEGER NOT NULL DEFAULT 0,
    change_seq    INTEGER NOT NULL,                -- collection.sync_token at write
    updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (collection_id, uid)
);

CREATE INDEX idx_resources_collection ON resources (collection_id);
CREATE INDEX idx_resources_range ON resources (collection_id, dtstart, dtend);
CREATE INDEX idx_resources_change ON resources (collection_id, change_seq);

-- Tombstones for RFC 6578 sync-collection.
CREATE TABLE deletions (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    uid           TEXT NOT NULL,
    change_seq    INTEGER NOT NULL,
    PRIMARY KEY (collection_id, uid)
);

CREATE INDEX idx_deletions_change ON deletions (collection_id, change_seq);
