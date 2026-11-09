-- v2: streams — TCP/UDP port forwarding via Angie's stream {} context.

CREATE TABLE streams (
    id             INTEGER PRIMARY KEY,
    incoming_port  INTEGER NOT NULL,               -- port Angie listens on
    forward_host   TEXT    NOT NULL,               -- upstream IP or hostname
    forward_port   INTEGER NOT NULL,
    tcp            INTEGER NOT NULL DEFAULT 1,
    udp            INTEGER NOT NULL DEFAULT 0,
    certificate_id INTEGER REFERENCES certificates (id) ON DELETE RESTRICT, -- optional TLS termination
    enabled        INTEGER NOT NULL DEFAULT 1,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
