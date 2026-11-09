-- API tokens: non-browser access to the same REST API (the `apctl` CLI, CI,
-- Ansible). Sessions stay cookie-based; tokens travel in `Authorization:
-- Bearer` and are never sent ambiently by a browser.
--
-- token_hash is a plain SHA-256 of the secret, NOT argon2 like `users`.
-- Deliberate: the secret is 256 bits from the CSPRNG, so it has no dictionary
-- to attack and needs no work factor — and argon2 (19 MiB, ~50 ms) would run on
-- every single API request. Argon2 stays where it belongs: human passwords.
--
-- user_id NULL = the machine-local token bootstrapped into the data dir for
-- `apctl` on the box itself. It has no owning account because it predates one:
-- root on this host can already read the DB and the setup token, so the file
-- grants nothing root did not already have.
CREATE TABLE api_tokens (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL,
    token_hash   TEXT    NOT NULL UNIQUE,  -- hex sha256 of the secret
    prefix       TEXT    NOT NULL,         -- first 8 chars, shown in the UI to tell tokens apart
    user_id      INTEGER REFERENCES users (id) ON DELETE CASCADE,
    is_local     INTEGER NOT NULL DEFAULT 0, -- the bootstrapped machine-local token
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER,
    expires_at   INTEGER                   -- NULL = never expires
);

CREATE INDEX idx_api_tokens_hash ON api_tokens (token_hash);
CREATE UNIQUE INDEX idx_api_tokens_local ON api_tokens (is_local) WHERE is_local = 1;
