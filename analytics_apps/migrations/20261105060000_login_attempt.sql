CREATE TABLE uac_login_attempt (
 user_id TEXT PRIMARY KEY REFERENCES uac_user(id) ON DELETE CASCADE,
 failures TEXT NOT NULL DEFAULT '0',
 next_delay TEXT NOT NULL DEFAULT '0',
 blocked_until TEXT NOT NULL DEFAULT '0'
);