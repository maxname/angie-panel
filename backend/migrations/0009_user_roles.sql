-- v2: multi-user with roles. `admin` = full control; `viewer` = read-only.
-- Existing single account becomes an admin (the safe default — never silently
-- demote the only operator). New users are created by an admin via /api/users.

ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'admin';
