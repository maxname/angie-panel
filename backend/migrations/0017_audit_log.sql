-- Audit log: one row per mutating request that reaches a handler (who, what,
-- outcome). Populated centrally by the security middleware; the table is
-- capped to the newest ~2000 rows on insert so it can't grow unbounded.
CREATE TABLE audit_log (
    id         INTEGER PRIMARY KEY,
    user_email TEXT,               -- NULL for unauthenticated requests (e.g. login)
    method     TEXT    NOT NULL,   -- POST / PUT / DELETE
    path       TEXT    NOT NULL,   -- request path, e.g. /api/hosts/5
    status     INTEGER NOT NULL,   -- response status code
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_audit_created ON audit_log (created_at DESC);
