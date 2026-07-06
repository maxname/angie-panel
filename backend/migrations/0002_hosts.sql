-- M1: proxy hosts + certificates schema (certificate behavior lands in M2),
-- apply history. Settings stay in the key/value table.

CREATE TABLE certificates (
    id           INTEGER PRIMARY KEY,
    -- acme_client name; immutable after creation (interpolated into
    -- $acme_cert_<name> variables): ^[a-z0-9_]{1,32}$
    name         TEXT    NOT NULL UNIQUE,
    domains      TEXT    NOT NULL,              -- JSON array, authoritative SAN
    challenge    TEXT    NOT NULL DEFAULT 'http',  -- http | dns | alpn
    key_type     TEXT    NOT NULL DEFAULT 'ecdsa', -- ecdsa | rsa
    email        TEXT,
    staging      INTEGER NOT NULL DEFAULT 0,
    status_cache TEXT,                          -- JSON, refreshed from /status API
    created_at   INTEGER NOT NULL
);

CREATE TABLE proxy_hosts (
    id                    INTEGER PRIMARY KEY,
    domains               TEXT    NOT NULL,      -- JSON array, idna-normalized
    forward_scheme        TEXT    NOT NULL DEFAULT 'http',
    forward_host          TEXT    NOT NULL,
    forward_port          INTEGER NOT NULL,
    websockets_upgrade    INTEGER NOT NULL DEFAULT 0,
    block_exploits        INTEGER NOT NULL DEFAULT 0,
    cache_assets          INTEGER NOT NULL DEFAULT 0,
    http2                 INTEGER NOT NULL DEFAULT 1,
    force_ssl             INTEGER NOT NULL DEFAULT 0,
    hsts                  INTEGER NOT NULL DEFAULT 0,
    hsts_subdomains       INTEGER NOT NULL DEFAULT 0,
    trust_forwarded_proto INTEGER NOT NULL DEFAULT 0,
    certificate_id        INTEGER REFERENCES certificates (id) ON DELETE RESTRICT,
    locations             TEXT    NOT NULL DEFAULT '[]',  -- JSON array
    advanced_snippet      TEXT,
    enabled               INTEGER NOT NULL DEFAULT 1,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL
);

CREATE TABLE apply_history (
    id          INTEGER PRIMARY KEY,
    timestamp   INTEGER NOT NULL,
    db_revision INTEGER NOT NULL,
    result      TEXT    NOT NULL,  -- ok | validation_failed | reload_failed | error
    report      TEXT    NOT NULL   -- full ApplyReport JSON (includes the diff)
);
