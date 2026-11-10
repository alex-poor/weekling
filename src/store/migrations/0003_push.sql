-- M4: WebDAV-Push (draft-bitfire-webdav-push) subscriptions.

CREATE TABLE push_subscriptions (
    id            TEXT PRIMARY KEY,           -- registration id (→ /dav/push/<id>)
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    principal_id  TEXT NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    push_resource TEXT NOT NULL,              -- the client's Web Push endpoint
    p256dh        TEXT NOT NULL,              -- subscription public key (base64url)
    auth_secret   TEXT NOT NULL,              -- auth secret (base64url)
    expires_at    TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (collection_id, push_resource)
);

CREATE INDEX idx_push_collection ON push_subscriptions (collection_id);
