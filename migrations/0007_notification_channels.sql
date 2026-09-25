-- PART 08 — per-user notification channels with encrypted config.
--
-- Each row is one channel a user has configured. The channel's
-- sensitive fields (bot tokens, webhook URLs, auth secrets) are
-- serialized as JSON and encrypted with ChaCha20-Poly1305 before
-- storage. The raw secret material is never exposed by any API
-- response.
--
-- Exactly one *enabled* channel per (user, channel_type) is allowed,
-- so the worker has an unambiguous destination when it dispatches a
-- reminder. Disabled channels may coexist (e.g. a user keeps a
-- Telegram channel configured but disabled while trying Discord).

CREATE TABLE notification_channels (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_type     TEXT        NOT NULL,
    name             TEXT        NOT NULL,
    encrypted_config BYTEA       NOT NULL,
    enabled          BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_tested_at   TIMESTAMPTZ,
    last_test_error  TEXT,

    CONSTRAINT channels_type_valid
        CHECK (channel_type IN ('telegram', 'discord', 'webhook')),
    CONSTRAINT channels_name_not_empty
        CHECK (length(btrim(name)) > 0),
    CONSTRAINT channels_name_max_length
        CHECK (length(name) <= 100),
    CONSTRAINT channels_config_not_empty
        CHECK (length(encrypted_config) > 0)
);

-- At most one enabled channel per (user, type).
CREATE UNIQUE INDEX idx_channels_user_type_enabled
    ON notification_channels (user_id, channel_type)
    WHERE enabled = TRUE;

-- Per-user listing.
CREATE INDEX idx_channels_user_id
    ON notification_channels (user_id);
