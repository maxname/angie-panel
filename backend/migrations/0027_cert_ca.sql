-- Certificate authority per certificate. Every existing row keeps Let's
-- Encrypt, which is what the generator hardcoded until now. The id refers to
-- the acme_cas registry; `custom` takes its directory URL from settings.
ALTER TABLE certificates ADD COLUMN ca TEXT NOT NULL DEFAULT 'letsencrypt';
