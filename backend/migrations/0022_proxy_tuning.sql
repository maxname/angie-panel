-- Per-host proxy fine-tuning. Stored as one JSON blob
-- {client_max_body_size, connect_timeout_secs, read_timeout_secs,
--  send_timeout_secs, disable_buffering}; NULL / absent = Angie defaults.
ALTER TABLE proxy_hosts ADD COLUMN proxy_tuning TEXT;
