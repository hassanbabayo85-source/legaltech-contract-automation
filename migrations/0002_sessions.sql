-- PART 03 — sessions table for server-side authentication.
--
-- A session is a server-side record that authenticates a user via an
-- opaque bearer token. The raw token is generated with 256 bits of
-- entropy and given to the client exactly once; only a SHA-256 hash of
-- the token is stored here. That means:
--
--   * A database leak does not let an attacker authenticate as any user
--     (they would need to reverse SHA-256 on a 256-bit random input).
--   * Lookups are a single indexed equality check on `token_hash`.
--
-- Why SHA-256 and not Argon2? Argon2 is a *password* KDF: it is
-- deliberately slow, to compensate for passwords having low entropy.
-- Session tokens already have 256 bits of entropy, so no amount of
-- slowness would improve them meaningfully, and Argon2's cost would make
-- every authenticated request prohibitively expensive. A fast
-- cryptographic hash is the correct primitive here.

CREATE TABLE sessions (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash    TEXT        NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sessions_token_hash_unique  UNIQUE (token_hash),
    CONSTRAINT sessions_token_hash_shape   CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT sessions_revoked_after_created CHECK (
        revoked_at IS NULL OR revoked_at >= created_at
    ),
    CONSTRAINT sessions_expires_after_created CHECK (expires_at > created_at)
);

-- Per-user listing: "show my active sessions" / bulk revocation.
CREATE INDEX idx_sessions_user_id ON sessions (user_id);

-- Session lookup happens on every authenticated request. The UNIQUE
-- constraint above already provides an index on token_hash.

-- Expiry cleanup: a future worker deletes rows where expires_at <= NOW()
-- or revoked_at is old enough. A partial index restricted to still-live
-- sessions keeps it small.
CREATE INDEX idx_sessions_active_expiry
    ON sessions (expires_at)
    WHERE revoked_at IS NULL;
