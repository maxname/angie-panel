-- Per-host maintenance mode: when enabled the host serves a styled 503 page
-- instead of proxying. Stored as one JSON blob {enabled, title, message};
-- NULL / absent = maintenance off.
ALTER TABLE proxy_hosts ADD COLUMN maintenance TEXT;
