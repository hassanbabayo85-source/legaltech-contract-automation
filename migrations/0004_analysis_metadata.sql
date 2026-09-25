-- PART 05 — AI analysis metadata on contracts.
--
-- Records who analyzed which version of a contract, when, and with what
-- prompt. All columns are nullable: a freshly created contract has no
-- analysis, and a failed analysis has only `analysis_error` populated.
--
-- `analyzed_content_version` is the content_version that was current
-- when the AI call started. If it differs from the contract's current
-- `content_version`, the analysis is stale for the current text.
--
-- `analysis_error` holds a short, safe category string (e.g. "timeout",
-- "rate_limited") — never a raw provider response, which could contain
-- request fragments.

ALTER TABLE contracts
    ADD COLUMN analyzed_at                TIMESTAMPTZ,
    ADD COLUMN analyzed_content_version   INTEGER,
    ADD COLUMN analysis_provider          TEXT,
    ADD COLUMN analysis_model             TEXT,
    ADD COLUMN analysis_prompt_version    INTEGER,
    ADD COLUMN analysis_error             TEXT;

ALTER TABLE contracts
    ADD CONSTRAINT contracts_analyzed_version_positive
        CHECK (analyzed_content_version IS NULL OR analyzed_content_version >= 1),
    ADD CONSTRAINT contracts_prompt_version_positive
        CHECK (analysis_prompt_version IS NULL OR analysis_prompt_version >= 1);
