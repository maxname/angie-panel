-- Per-host gzip compression. Stored as one JSON blob
-- {enabled, comp_level, min_length, types}; NULL / absent = gzip off.
ALTER TABLE proxy_hosts ADD COLUMN gzip TEXT;
