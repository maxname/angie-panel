-- Named DNS provider credential profiles. Lets several accounts of the SAME
-- provider type coexist (e.g. two Cloudflare tokens). A certificate's
-- dns_provider now references a profile id (as text) rather than a provider
-- type; the secret values live in the settings table under
-- dns_cred:<profile_id>:<ENV>. `provider` is the type id from the registry
-- (cloudflare, regru, …). `name` is the operator's label.
CREATE TABLE dns_credentials (
    id         INTEGER PRIMARY KEY,
    provider   TEXT    NOT NULL,   -- provider TYPE id (dns_providers registry)
    name       TEXT    NOT NULL,   -- UI label, e.g. "Cloudflare — work"
    created_at INTEGER NOT NULL
);
