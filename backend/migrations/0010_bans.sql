-- v2: IP blocklist ("banned IPs"). A global deny list generated into Angie at
-- http scope (03-bans.conf), so banned addresses get 403 on every host that
-- doesn't define its own access rules. This is the panel-native enforcement
-- point that fail2ban / CrowdSec can drive (push a ban → apply).

CREATE TABLE bans (
    id         INTEGER PRIMARY KEY,
    address    TEXT    NOT NULL UNIQUE,  -- bare IP or IP/CIDR (v4 or v6)
    reason     TEXT,                     -- optional note (UI metadata only)
    created_at INTEGER NOT NULL
);
