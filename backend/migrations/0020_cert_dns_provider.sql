-- v2: DNS-01 via a provider API (ACME hook). NULL = Angie answers DNS itself
-- (NS delegation, acme_dns_port). A value (e.g. 'regru') = the panel's ACME
-- hook creates the _acme-challenge TXT via that provider's API — automatic
-- wildcard with no inbound UDP/53.
ALTER TABLE certificates ADD COLUMN dns_provider TEXT;
