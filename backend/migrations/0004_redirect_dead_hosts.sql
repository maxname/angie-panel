-- v2: redirection hosts and 404 (dead) hosts — additional host types beyond
-- proxy hosts, matching nginx-proxy-manager parity.

CREATE TABLE redirect_hosts (
    id               INTEGER PRIMARY KEY,
    domains          TEXT    NOT NULL,               -- JSON array
    forward_scheme   TEXT    NOT NULL DEFAULT 'auto', -- auto | http | https (target scheme)
    forward_domain   TEXT    NOT NULL,               -- where to redirect to
    forward_http_code INTEGER NOT NULL DEFAULT 301,   -- 300-308
    preserve_path    INTEGER NOT NULL DEFAULT 1,
    certificate_id   INTEGER REFERENCES certificates (id) ON DELETE RESTRICT,
    force_ssl        INTEGER NOT NULL DEFAULT 0,
    hsts             INTEGER NOT NULL DEFAULT 0,
    hsts_subdomains  INTEGER NOT NULL DEFAULT 0,
    http2            INTEGER NOT NULL DEFAULT 1,
    block_exploits   INTEGER NOT NULL DEFAULT 0,
    advanced_snippet TEXT,
    enabled          INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE dead_hosts (
    id               INTEGER PRIMARY KEY,
    domains          TEXT    NOT NULL,               -- JSON array
    certificate_id   INTEGER REFERENCES certificates (id) ON DELETE RESTRICT,
    force_ssl        INTEGER NOT NULL DEFAULT 0,
    hsts             INTEGER NOT NULL DEFAULT 0,
    hsts_subdomains  INTEGER NOT NULL DEFAULT 0,
    http2            INTEGER NOT NULL DEFAULT 1,
    advanced_snippet TEXT,
    enabled          INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);
