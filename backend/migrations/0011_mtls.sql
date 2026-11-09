-- v2: mutual TLS (client certificates) per proxy host. Stored as one JSON
-- object (like rate_limit / upstream): {"ca_pem":"-----BEGIN...","optional":false}
-- NULL / absent = no client-cert requirement. Only takes effect on HTTPS hosts
-- (the CA verifies presented client certs; `optional` requests but doesn't
-- require one). The CA PEM is materialized into a managed http.d file at apply.

ALTER TABLE proxy_hosts ADD COLUMN mtls TEXT;
