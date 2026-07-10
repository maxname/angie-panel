-- v2: access lists (basic auth + IP allow/deny), attachable to proxy hosts.

CREATE TABLE access_lists (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    satisfy    TEXT    NOT NULL DEFAULT 'all',  -- 'any' | 'all'
    pass_auth  INTEGER NOT NULL DEFAULT 0,      -- pass the Authorization header upstream
    created_at INTEGER NOT NULL
);

CREATE TABLE access_list_users (
    id             INTEGER PRIMARY KEY,
    access_list_id INTEGER NOT NULL REFERENCES access_lists (id) ON DELETE CASCADE,
    username       TEXT    NOT NULL,
    password_hash  TEXT    NOT NULL              -- bcrypt
);

CREATE TABLE access_list_clients (
    id             INTEGER PRIMARY KEY,
    access_list_id INTEGER NOT NULL REFERENCES access_lists (id) ON DELETE CASCADE,
    directive      TEXT    NOT NULL,             -- 'allow' | 'deny'
    address        TEXT    NOT NULL              -- IP / CIDR / 'all'
);

-- Hosts reference at most one access list; detaching on delete keeps the host.
ALTER TABLE proxy_hosts
    ADD COLUMN access_list_id INTEGER REFERENCES access_lists (id) ON DELETE SET NULL;
