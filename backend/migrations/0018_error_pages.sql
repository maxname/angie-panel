-- Per-host custom error pages. Stored as one JSON blob
-- {not_found:{enabled,title,message}, server_error:{enabled,title,message}};
-- NULL / absent = no custom pages (Angie serves its defaults).
ALTER TABLE proxy_hosts ADD COLUMN error_pages TEXT;
