-- Shared upstream pools: a named set of backend servers (plus balancing method
-- and passive health) defined once and referenced by any number of proxy
-- hosts, instead of repeating the same servers in every host. A host that
-- references a pool proxies to it INSTEAD of its own forward_host:forward_port.
--
-- `servers` is one JSON array, the same shape as a host's own extra servers:
-- [{"host":"10.0.0.2","port":8080,"weight":1,"backup":false,"down":false}]
CREATE TABLE upstream_pools (
    id                INTEGER PRIMARY KEY,
    name              TEXT    NOT NULL UNIQUE,                 -- UI label, also a config comment
    servers           TEXT    NOT NULL DEFAULT '[]',           -- JSON array, see above
    method            TEXT    NOT NULL DEFAULT 'round_robin',  -- round_robin | least_conn | ip_hash
    max_fails         INTEGER NOT NULL DEFAULT 1,
    fail_timeout_secs INTEGER NOT NULL DEFAULT 10,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

-- RESTRICT, not SET NULL: a host whose pool vanished would be left with no
-- backend at all. The API refuses to delete a pool that is in use; this is the
-- backstop behind it.
ALTER TABLE proxy_hosts
    ADD COLUMN upstream_pool_id INTEGER REFERENCES upstream_pools (id) ON DELETE RESTRICT;
