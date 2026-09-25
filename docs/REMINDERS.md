# Reminder engine

PART 06 turns contract deadlines and obligation due dates into scheduled
reminder records. It does not send notifications. Delivery is
implemented in a later part.

## Reminder types

| Type | Offset from deadline |
|---|---|
| 7_days_before | deadline minus 7 days |
| 3_days_before | deadline minus 3 days |
| 1_day_before | deadline minus 1 day |
| on_deadline | deadline |
| custom | user-chosen timestamp |

## Channels

telegram, discord, webhook. The engine records the intended channel;
nothing is dispatched yet.

## Status lifecycle

    pending --> processing --> sent
                         --> failed
    pending --> cancelled

Only pending -> cancelled is exposed via the API. The other
transitions belong to the future dispatcher.

## Standard schedule

Every DATE deadline generates up to four system reminders, one for
each offset above, timestamped at 00:00:00 UTC on the computed date.

Past-deadline policy (today = UTC date):

| Deadline relative to today | Reminders generated |
|---|---|
| in the past | none |
| today | on_deadline |
| tomorrow | 1_day_before, on_deadline |
| in 2 days | 1_day_before, on_deadline |
| in 3 days | 3_days_before, 1_day_before, on_deadline |
| in 7 days | all four |
| beyond 7 days | all four |

An offset whose computed date is strictly before today is skipped.
This is documented behavior, not silent truncation.

## Timezone policy

- Deadlines and due dates are stored as DATE.
- Reminder timestamps are TIMESTAMPTZ.
- The conversion is fixed at 00:00:00 UTC on the computed date, so
  the same input produces the same output regardless of the server's
  local timezone. date_to_utc_midnight is the only place where a
  DATE becomes a TIMESTAMPTZ.

## Idempotency

POST /api/contracts/:id/reminders/generate is idempotent. Idempotency
is enforced by PostgreSQL, not by application-level checks. Two
partial unique indexes cover obligation-level and contract-level
system reminders:

- (contract_id, obligation_id, reminder_date, reminder_type, channel_type)
  WHERE reminder_source = 'system' AND obligation_id IS NOT NULL
- (contract_id, reminder_date, reminder_type, channel_type)
  WHERE reminder_source = 'system' AND obligation_id IS NULL

Two indexes are needed because PostgreSQL treats NULL as distinct in
UNIQUE constraints; a single index would let contract-level reminders
duplicate.

## Generated vs custom

System reminders (reminder_source = 'system') may be deleted and
replaced during regeneration. Custom reminders (reminder_source =
'custom') are preserved indefinitely unless the user cancels them.
Custom reminders are NOT deduplicated - a user may legitimately want
two reminders at the same instant on different channels.

## Analysis-version linkage

System reminders record source_content_version - the contract's
content_version when they were generated. Before generating, the
service requires:

- analysis_status = 'completed'
- analyzed_content_version = content_version

If either fails, the endpoint returns 409 Conflict. This is how the
engine refuses to schedule reminders against stale obligations.

## Regeneration

POST /api/contracts/:id/reminders/generate runs in a single
transaction:

1. Delete pending system reminders for the contract.
2. Insert the computed system reminders (ON CONFLICT DO NOTHING).
3. Write an audit entry.

Custom reminders, and system reminders in any non-pending state
(cancelled, failed, sent), are untouched.

## API endpoints

| Method | Path | Purpose |
|---|---|---|
| POST | /api/contracts/:id/reminders/generate | Regenerate system reminders |
| POST | /api/contracts/:id/reminders | Create a custom reminder |
| GET | /api/contracts/:id/reminders | List reminders (paginated) |
| POST | /api/reminders/:id/cancel | Cancel a pending reminder |

All require Authorization: Bearer token.

Custom reminder body:

    {
      "obligation_id": "uuid or null",
      "reminder_date": "2026-10-20T09:00:00Z",
      "channel_type": "telegram"
    }

obligation_id must belong to the same contract; cross-contract
references are rejected (and would also be caught by the composite
foreign key).

## Ownership

Every endpoint joins through contracts.user_id. Nonexistent and
foreign resources both return 404 Not Found.

## State transitions

Only pending -> cancelled is exposed. Any other transition attempt
returns 409 Conflict. Clients cannot set status directly
(deny_unknown_fields).

## Audit events

| Action | Metadata |
|---|---|
| contract_reminders_regenerated | created, skipped_duplicates, preserved_custom, deleted_pending_system, content_version |
| reminder_created | contract_id, obligation_id, channel_type |
| reminder_cancelled | contract_id |

No contract text is ever written to audit metadata.

## Future dispatcher (PART 07)

The dispatcher can identify work with:

    SELECT id FROM reminders
     WHERE status = 'pending'
       AND reminder_date <= NOW()
     ORDER BY reminder_date
     FOR UPDATE SKIP LOCKED;

The partial index idx_reminders_pending_due (from PART 02) covers
this query.
