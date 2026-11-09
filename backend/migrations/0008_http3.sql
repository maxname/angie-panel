-- v2: HTTP/3 (QUIC) support per proxy host. Adds a `listen 443 quic;` +
-- `http3 on;` + Alt-Svc advertisement alongside the TLS listener. Only takes
-- effect on HTTPS hosts (QUIC is always encrypted). No `reuseport` is emitted:
-- it may appear at most once per address, and coupling independent per-host
-- files would be fragile — plain `listen 443 quic;` shares the socket fine.

ALTER TABLE proxy_hosts ADD COLUMN http3 INTEGER NOT NULL DEFAULT 0;
