-- Per-host availability checks, plus the heartbeats they produce.
--
-- Angie cannot do this for us: active `health_check` is a PRO feature, and the
-- passive max_fails/fail_timeout we do have only reacts to real traffic — a host
-- nobody visits stays "up" forever. So the panel polls, and stores what it saw.
--
-- The config rides on the host as one JSON array, like `locations` and
-- `rate_limit`: [{"kind":"tcp","enabled":true,"interval_secs":null,...}]
-- NULL / absent = no checks. A null interval/timeout means "inherit the app
-- default" from settings, so changing the default moves every host that never
-- overrode it.
ALTER TABLE proxy_hosts ADD COLUMN health_checks TEXT;

-- One row per check attempt. Kept flat and append-only: the UI reads the last N
-- for a host+kind and nothing else, so there is no shape here worth normalising.
--
-- No FK to proxy_hosts: beats outlive their host on purpose — deleting a host
-- must not block on a sweep of its history. The reaper below clears orphans.
CREATE TABLE health_beats (
    host_id    INTEGER NOT NULL,
    kind       TEXT    NOT NULL,          -- 'tcp' | 'http'
    ts         INTEGER NOT NULL,          -- unix seconds
    ok         INTEGER NOT NULL,          -- 1 = up
    latency_ms INTEGER,                   -- NULL when the attempt never connected
    error      TEXT                       -- NULL when ok
);

-- The only read pattern: "last N beats for this host+kind, newest first".
CREATE INDEX idx_health_beats_lookup ON health_beats (host_id, kind, ts DESC);
