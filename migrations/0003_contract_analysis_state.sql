-- PART 04 — analysis state tracking on contracts.
--
-- When a contract's raw_text changes, any AI analysis produced against
-- the previous text becomes stale. Without a version/state pair the
-- database cannot tell whether a stored risk summary was produced for
-- the current text or a previous revision.
--
--   * content_version  — starts at 1, increments on every raw_text
--                        change. A future AI pipeline records the
--                        version it analyzed; a mismatch signals stale
--                        analysis.
--   * analysis_status  — coarse state of the pipeline:
--                          not_analyzed -> no analysis for the current
--                                          content_version
--                          pending      -> queued / in progress
--                          completed    -> analysis current for the
--                                          current content_version
--                          failed       -> last attempt errored
--
-- Both columns are NOT NULL with safe defaults, so existing rows
-- migrate cleanly without a backfill.

ALTER TABLE contracts
    ADD COLUMN content_version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN analysis_status TEXT    NOT NULL DEFAULT 'not_analyzed';

ALTER TABLE contracts
    ADD CONSTRAINT contracts_content_version_positive
        CHECK (content_version >= 1),
    ADD CONSTRAINT contracts_analysis_status_valid
        CHECK (analysis_status IN ('not_analyzed', 'pending', 'completed', 'failed'));
