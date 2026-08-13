-- Password reset.
--
-- Reset tokens are stored as a SHA-256 hash rather than in the clear: the token
-- is a bearer credential, so a leaked backup of this table must not be enough to
-- take over an account. The raw token exists only in the email.

CREATE TABLE password_reset_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash CHAR(64) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    -- Set when the token is spent, so a reset link works exactly once.
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_password_reset_tokens_user ON password_reset_tokens(user_id);
CREATE INDEX idx_password_reset_tokens_expires ON password_reset_tokens(expires_at);

-- Generation counter stamped into every token this user is issued. Changing the
-- password bumps it, which invalidates every token carrying the old value —
-- something a per-token denylist cannot express, because the sessions to end
-- are not known at reset time.
--
-- A counter rather than a timestamp: a JWT `iat` claim only has second
-- resolution, so a cut-off time cannot separate a token issued just before a
-- reset from one issued just after it within the same second.
ALTER TABLE users
    ADD COLUMN session_epoch INTEGER NOT NULL DEFAULT 0;
