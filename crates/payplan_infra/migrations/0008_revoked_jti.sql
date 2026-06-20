-- Track C: tracks revoked JWT IDs so logout and refresh-token rotation can
-- invalidate tokens before their natural expiry. Rows are safe to purge
-- after `expires_at` (the token would have been rejected anyway).
CREATE TABLE revoked_jti (
    jti         TEXT PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id),
    token_type  TEXT NOT NULL CHECK (token_type IN ('access', 'refresh')),
    revoked_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX revoked_jti_expires_at_idx ON revoked_jti(expires_at);
