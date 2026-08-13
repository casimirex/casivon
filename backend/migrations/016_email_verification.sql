-- Email verification, finally.
--
-- `users.email_verified` has existed since `001_create_users.sql` and has never
-- once been set to true. The column has been advertising a feature that was not
-- there — anyone reading the schema would reasonably assume addresses were
-- confirmed, and they were not.
--
-- It was blocked on having no way to send mail. That stopped being true when
-- SMTP landed, so this closes it.

-- Same shape as `password_reset_tokens`, and deliberately so: both are
-- single-use bearer credentials delivered by email, and the reasoning that
-- applies to one applies to the other. Stored as a SHA-256 hash rather than in
-- the clear, so a leaked backup of this table is not enough to confirm somebody
-- else's address; the raw token exists only in the email.
CREATE TABLE email_verification_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash CHAR(64) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    -- Set when spent, so a verification link works exactly once.
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_email_verification_tokens_user ON email_verification_tokens(user_id);
CREATE INDEX idx_email_verification_tokens_expires ON email_verification_tokens(expires_at);

-- --------------------------------------------------- existing accounts stay
--                                                      unverified
--
-- No `UPDATE users SET email_verified = true`, tempting as it is to spare
-- everyone the prompt.
--
-- Those addresses have never been confirmed. Marking them verified would make
-- the column assert something nobody checked — which is precisely the problem
-- this migration exists to fix, just written the other way round. They can
-- verify from the prompt whenever they like.
--
-- Nothing is gated on it. Verification is recorded and surfaced, and sign-in
-- works exactly as before; requiring it would lock out every existing account
-- the moment this shipped, which is not a decision a migration should make on
-- an operator's behalf.
