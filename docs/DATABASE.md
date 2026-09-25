# Database

PostgreSQL schema owned by `migrations/` and the decisions behind it.

## Technology

* PostgreSQL 13+ (`gen_random_uuid()`).
* **SQLx 0.8 or newer.** This is a hard requirement on PostgreSQL 15+.
  SQLx 0.7 runs `LOCK TABLE pg_catalog.pg_namespace IN SHARE ROW
  EXCLUSIVE MODE` inside `#[sqlx::test]` setup, which requires
  `SUPERUSER` on PostgreSQL 18 (and is rejected with SQLSTATE `42501`
  otherwise). SQLx 0.8 removes that LOCK. Verified against PostgreSQL
  18.6 on 2026-09-23.
* No ORM.
* Migrations are SQL files under `migrations/`, applied by `sqlx-cli`.

## Applying migrations

    export DATABASE_URL="postgres://lexhack:lexhack_dev_pw@127.0.0.1:5432/lexhack_test"
    sqlx migrate run
    sqlx migrate info

A fresh database reaches the complete schema with one `sqlx migrate run`.

## Test database setup

    sudo -u postgres psql -c "DROP DATABASE IF EXISTS lexhack_test;"
    sudo -u postgres psql -c "DROP USER IF EXISTS lexhack;"
    sudo -u postgres psql -c "CREATE USER lexhack WITH PASSWORD 'lexhack_dev_pw' CREATEDB;"
    sudo -u postgres psql -c "CREATE DATABASE lexhack_test OWNER lexhack;"
    sudo -u postgres psql -d lexhack_test -c "GRANT ALL ON SCHEMA public TO lexhack;"

`CREATEDB` is required because integration tests use `#[sqlx::test]`,
which creates and drops an isolated database per test. `SUPERUSER` is
explicitly NOT required (with SQLx 0.8 or newer).

## Table overview

    users
      |
      +-- contracts
            +-- contract_risks
            +-- contract_obligations
            |     +-- reminders (optional link)
            +-- reminders

    audit_logs  (references users, survives user deletion with NULL)

| Table | Purpose |
|---|---|
| users | Accounts. Password stored only as a hash. |
| contracts | Uploaded contracts. AI fields nullable until analysis. |
| contract_obligations | Discrete obligations/deadlines. |
| contract_risks | Individual risks. |
| reminders | Scheduled dispatcher work items. |
| audit_logs | Traceability events. |

## Controlled vocabularies: TEXT + CHECK, not ENUM

`risk_level`, `status`, `reminder_type`, `channel_type` are TEXT with
CHECK constraints. PostgreSQL ENUMs were rejected because they cannot
have values removed and force a type rewrite on change.

## Foreign keys and deletion policy

| Child | Parent | On delete | Why |
|---|---|---|---|
| contracts.user_id | users.id | CASCADE | Contract with no owner is meaningless. |
| contract_obligations.contract_id | contracts.id | CASCADE | Obligation has no meaning outside contract. |
| contract_risks.contract_id | contracts.id | CASCADE | Same. |
| reminders.contract_id | contracts.id | CASCADE | Reminder to a deleted contract is a bug. |
| reminders.(obligation_id, contract_id) | contract_obligations.(id, contract_id) | CASCADE | Obligation must belong to same contract. |
| audit_logs.user_id | users.id | SET NULL | Audit history must survive. |

The composite FK on reminders uses MATCH SIMPLE (default), so it is
not enforced when obligation_id IS NULL — allowing contract-level
reminders without a specific obligation.

## Reminder lifecycle

pending -> processing -> sent
                      \-> failed
                      \-> pending (retry)
cancelled is terminal.

The future dispatcher selects:
    status = 'pending' AND reminder_date <= NOW()
    FOR UPDATE SKIP LOCKED

The partial index idx_reminders_pending_due exists for this query.

## Sensitive data handling

* contracts.raw_text: never log, never put in audit metadata.
* users.password_hash: never log, never return.
* audit_logs.metadata: never contains passwords, tokens, API keys,
  cookies, or raw contract text.

## Adding a migration

    sqlx migrate add descriptive_name
    # edit migrations/<timestamp>_descriptive_name.sql
    sqlx migrate run

Never edit an applied migration; add a new one instead.

## Version history notes

* 2026-09-23 — sqlx upgraded from 0.7.4 to 0.8.6 to fix a
  PostgreSQL 18 incompatibility in `#[sqlx::test]` (the LOCK on
  pg_namespace). No application code changes were required for the
  upgrade; only the Cargo.toml version constraint was bumped.
