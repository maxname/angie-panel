-- v2: per-host rate limiting (Angie limit_req / limit_conn).
--
-- Stored as one JSON object (like `locations`) rather than flat columns:
-- {"enabled":true,"rps":10,"burst":20,"nodelay":false,"conn":5}
-- NULL / absent = disabled. Keeps the migration and repo plumbing minimal.

ALTER TABLE proxy_hosts ADD COLUMN rate_limit TEXT;
