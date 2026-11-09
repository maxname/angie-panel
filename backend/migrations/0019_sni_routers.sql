-- v2: SNI passthrough routers — one stream listener that routes TLS connections
-- to different backends by the SNI hostname in the ClientHello (ssl_preread),
-- WITHOUT terminating TLS. Routes are stored as one JSON array
-- [{sni, forward_host, forward_port}]; an optional catch-all backend handles
-- unmatched / absent SNI (empty host / 0 port = no catch-all → drop).
CREATE TABLE sni_routers (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL,               -- UI label (also a config comment)
    incoming_port INTEGER NOT NULL,               -- stream port Angie listens on
    routes        TEXT    NOT NULL DEFAULT '[]',  -- JSON: [{sni, forward_host, forward_port}]
    default_host  TEXT    NOT NULL DEFAULT '',    -- catch-all backend host ('' = none)
    default_port  INTEGER NOT NULL DEFAULT 0,     -- catch-all backend port (0 = none)
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
