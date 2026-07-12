-- Stream TLS termination mode. The `certificate_id` column already exists
-- (streams table, migration 0005) — this adds the mode selector: 'none'
-- (default) is the existing plain L4 forward, 'terminate' decrypts on the
-- incoming port with the referenced panel-managed certificate.
ALTER TABLE streams ADD COLUMN tls TEXT NOT NULL DEFAULT 'none';
