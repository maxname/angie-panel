-- v2: load balancing — multiple backend servers per host + balancing method +
-- passive health checks (max_fails/fail_timeout). Active health checks
-- (health_check/probe) are Angie PRO only, so OSS relies on passive failover.
--
-- Stored as one JSON object (like `locations` / `rate_limit`):
-- {"servers":[{"host":"10.0.0.2","port":8080,"weight":1,"backup":false,"down":false}],
--  "method":"round_robin","primary_weight":1,"max_fails":1,"fail_timeout_secs":10}
-- NULL / absent = a plain single-server host (the primary forward_host:port).

ALTER TABLE proxy_hosts ADD COLUMN upstream TEXT;
