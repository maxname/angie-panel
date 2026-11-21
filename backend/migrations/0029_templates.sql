-- Templates: a saved set of settings for a proxy host or a stream, to start a
-- new one from instead of filling the editor in again. A template is a preset,
-- not a link: what is created from it is an ordinary host/stream, and later
-- edits to the template do not reach it.
--
-- `config` is the create payload of that kind, validated by the same rules,
-- minus what makes each one unique — a host's domains, a stream's incoming
-- port. The forward target may be left blank too ("ask when creating").
CREATE TABLE templates (
    id         INTEGER PRIMARY KEY,
    kind       TEXT    NOT NULL,   -- 'proxy_host' | 'stream'
    name       TEXT    NOT NULL,   -- UI label; unique within a kind
    config     TEXT    NOT NULL,   -- JSON object, see above
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (kind, name)
);
