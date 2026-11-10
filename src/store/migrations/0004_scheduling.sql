-- M5: CalDAV scheduling (RFC 6638). A per-principal scheduling inbox is modelled
-- as a collection with role = 'inbox'; iTIP messages are delivered there.

ALTER TABLE collections ADD COLUMN role TEXT; -- NULL (normal) | 'inbox'
