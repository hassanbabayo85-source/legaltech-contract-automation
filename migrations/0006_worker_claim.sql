-- PART 07 — worker claim tracking on reminders.
--
-- The background worker must be able to:
--   * claim due reminders atomically (see the UPDATE ... WHERE
--     status='pending' pattern in src/worker/claim.rs),
--   * recover reminders abandoned by a crashed worker after a
--     configurable timeout,
--   * schedule the next retry attempt after a retryable failure.
--
-- Four columns support those needs. All are nullable so existing rows
-- migrate cleanly.
--
--   * claimed_at      — when the current claim started. Used by the
--                       crash-recovery sweep: any `processing` row
--                       whose `claimed_at` is older than
--                       WORKER_CLAIM_TIMEOUT_SECONDS is reset to
--                       'pending'.
--   * claimed_by      — a stable worker identifier (host + pid +
--                       short random suffix). Lets an operator see
--                       which instance holds a claim, and lets the
--                       outcome UPDATE verify it is still the owner.
--   * next_attempt_at — when a retry becomes eligible. The due-query
--                       requires `next_attempt_at IS NULL OR
--                       next_attempt_at <= NOW()`. NULL means "no
--                       scheduled retry; the reminder is due as soon
--                       as its reminder_date passes".
--   * last_attempt_at — when the most recent delivery attempt started.
--                       Diagnostic only.

ALTER TABLE reminders
    ADD COLUMN claimed_at       TIMESTAMPTZ,
    ADD COLUMN claimed_by       TEXT,
    ADD COLUMN next_attempt_at  TIMESTAMPTZ,
    ADD COLUMN last_attempt_at  TIMESTAMPTZ;

-- Partial index optimised for the worker's hot query:
--
--   SELECT id FROM reminders
--    WHERE status = 'pending'
--      AND reminder_date <= NOW()
--      AND (next_attempt_at IS NULL OR next_attempt_at <= NOW())
--    ORDER BY reminder_date, id
--    FOR UPDATE SKIP LOCKED
--    LIMIT $batch
--
-- Restricting to status='pending' keeps the index small (only rows
-- the worker cares about). The key order (next_attempt_at, reminder_date,
-- id) matches the sort so the planner can walk the index directly.
CREATE INDEX idx_reminders_worker_due
    ON reminders (next_attempt_at, reminder_date, id)
    WHERE status = 'pending';

-- Partial index for the crash-recovery sweep:
--
--   SELECT id FROM reminders
--    WHERE status = 'processing' AND claimed_at < NOW() - $timeout
--
-- The set of `processing` rows is normally tiny, so this index costs
-- almost nothing to maintain.
CREATE INDEX idx_reminders_processing_claimed_at
    ON reminders (claimed_at)
    WHERE status = 'processing';
