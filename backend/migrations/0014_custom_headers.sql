-- Per-host custom headers: a JSON array of {name, value, direction} added to
-- the host's responses (add_header) or upstream requests (proxy_set_header).
-- NULL / absent = no custom headers.
ALTER TABLE proxy_hosts ADD COLUMN custom_headers TEXT;
