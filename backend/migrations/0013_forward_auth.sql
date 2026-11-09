-- Forward authentication (SSO gateway) via Angie's auth_request. Stored as one
-- JSON blob on the host (enabled, verify_url, sign_in_url, copy_headers);
-- NULL / absent = no forward auth.
ALTER TABLE proxy_hosts ADD COLUMN forward_auth TEXT;
