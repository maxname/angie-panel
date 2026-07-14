-- Migration 0021 redefined certificates.dns_provider: it used to hold a
-- provider TYPE string (e.g. 'regru', added in 0020) and now references a DNS
-- credential PROFILE id (as text) from the dns_credentials table. Any value
-- carried over from a pre-0021 install is a type string that no longer maps to
-- anything, and the ACME hook would look up a non-existent profile.
--
-- Reset those orphans to NULL so the operator re-selects a credential profile.
-- Values that already reference an existing profile id are left untouched, so
-- this is a no-op on a fresh install and on correctly-migrated data.
UPDATE certificates
   SET dns_provider = NULL
 WHERE dns_provider IS NOT NULL
   AND dns_provider NOT IN (SELECT CAST(id AS TEXT) FROM dns_credentials);
