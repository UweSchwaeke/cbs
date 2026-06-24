-- Track first login. NULL means provisioned-but-never-logged-in (design 020).
ALTER TABLE users ADD COLUMN first_login_at INTEGER;

-- Backfill existing human rows: a login-created row sets created_at at its
-- first login (the login upsert INSERTs the row on first auth and never
-- rewrites created_at), so created_at is an accurate first-login time for
-- them. Robots never log in and stay NULL. A never-logged-in seed_admin is
-- marginally un-pended; this is accepted (design 020 § Migration &
-- Compatibility).
UPDATE users SET first_login_at = created_at WHERE is_robot = 0;
