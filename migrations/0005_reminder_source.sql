-- PART 06 — reminder source & analysis-version linkage.

ALTER TABLE reminders
    ADD COLUMN reminder_source        TEXT    NOT NULL DEFAULT 'system',
    ADD COLUMN source_content_version INTEGER;

ALTER TABLE reminders
    ADD CONSTRAINT reminders_source_valid
        CHECK (reminder_source IN ('system', 'custom')),
    ADD CONSTRAINT reminders_source_version_positive
        CHECK (source_content_version IS NULL OR source_content_version >= 1);

CREATE UNIQUE INDEX idx_reminders_system_obligation_unique
    ON reminders (contract_id, obligation_id, reminder_date, reminder_type, channel_type)
    WHERE reminder_source = 'system' AND obligation_id IS NOT NULL;

CREATE UNIQUE INDEX idx_reminders_system_contract_unique
    ON reminders (contract_id, reminder_date, reminder_type, channel_type)
    WHERE reminder_source = 'system' AND obligation_id IS NULL;
