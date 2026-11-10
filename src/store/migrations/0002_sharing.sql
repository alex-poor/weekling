-- M3: calendar sharing + task (VTODO) indexing.

-- Component type of each object, so calendar-query can filter VEVENT vs VTODO.
ALTER TABLE resources ADD COLUMN component TEXT; -- 'VEVENT' | 'VTODO' | NULL

-- Per-calendar grants to another principal.
CREATE TABLE shares (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    grantee_id    TEXT NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    access        TEXT NOT NULL CHECK (access IN ('read', 'read-write', 'freebusy')),
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (collection_id, grantee_id)
);

CREATE INDEX idx_shares_grantee ON shares (grantee_id);
