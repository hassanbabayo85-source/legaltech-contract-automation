//! Integration tests for the PART 02 database schema.
//!
//! These tests run against a real PostgreSQL instance. They use
//! `#[sqlx::test]`, which for each test:
//!
//! 1. Connects to the database named in `DATABASE_URL`.
//! 2. Creates a fresh, uniquely named database.
//! 3. Applies every migration under `./migrations`.
//! 4. Hands the test a `PgPool` bound to that fresh database.
//! 5. Drops the database when the test finishes.
//!
//! As a result, tests never share state, never need manual cleanup, and
//! can run in parallel. If `DATABASE_URL` is not set, or PostgreSQL is
//! unreachable, the tests **fail loudly** — they never silently skip. A
//! green `cargo test` therefore always means the schema was exercised.

use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use lexhack_backend::db;

async fn seed_user(pool: &PgPool) -> Uuid {
    db::users::insert_user(
        pool,
        "alice@example.com",
        "$argon2id$v=19$m=19456,t=2,p=1$placeholder$placeholder",
        "Alice Example",
    )
    .await
    .expect("seed user should insert")
    .id
}

async fn seed_contract(pool: &PgPool, user_id: Uuid) -> Uuid {
    db::contracts::insert_contract(
        pool,
        user_id,
        "Employment Agreement",
        "Full contract text placeholder.",
    )
    .await
    .expect("seed contract should insert")
    .id
}

fn assert_db_error_code(err: &sqlx::Error, expected_code: &str, context: &str) {
    let db_err = err
        .as_database_error()
        .unwrap_or_else(|| panic!("{context}: expected a database error, got: {err}"));
    assert_eq!(
        db_err.code().as_deref(),
        Some(expected_code),
        "{context}: unexpected SQLSTATE (message: {})",
        db_err.message()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn user_can_be_inserted(pool: PgPool) {
    let user = db::users::insert_user(
        &pool,
        "alice@example.com",
        "$argon2id$v=19$m=19456,t=2,p=1$placeholder$placeholder",
        "Alice Example",
    )
    .await
    .expect("user insert should succeed");

    assert_eq!(user.email, "alice@example.com");
    assert_eq!(user.full_name, "Alice Example");
    assert!(user.created_at <= Utc::now());
    assert_eq!(user.created_at, user.updated_at);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_email_is_rejected(pool: PgPool) {
    seed_user(&pool).await;

    let err = db::users::insert_user(
        &pool,
        "alice@example.com",
        "$argon2id$v=19$m=19456,t=2,p=1$placeholder$placeholder",
        "Another Alice",
    )
    .await
    .expect_err("duplicate email must be rejected");

    assert_db_error_code(&err, "23505", "duplicate email");
}

#[sqlx::test(migrations = "./migrations")]
async fn empty_email_is_rejected(pool: PgPool) {
    let err = db::users::insert_user(
        &pool,
        "   ",
        "$argon2id$v=19$m=19456,t=2,p=1$placeholder$placeholder",
        "Alice Example",
    )
    .await
    .expect_err("empty email must be rejected");

    assert_db_error_code(&err, "23514", "empty email");
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_can_be_inserted_with_nullable_ai_fields(pool: PgPool) {
    let user_id = seed_user(&pool).await;

    let contract = db::contracts::insert_contract(
        &pool,
        user_id,
        "Employment Agreement",
        "Full contract text placeholder.",
    )
    .await
    .expect("contract insert should succeed");

    assert_eq!(contract.user_id, user_id);
    assert_eq!(contract.title, "Employment Agreement");
    assert!(contract.start_date.is_none());
    assert!(contract.end_date.is_none());
    assert!(contract.risk_level.is_none());
    assert!(contract.risk_score.is_none());
    assert!(contract.risk_summary.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_requires_existing_user(pool: PgPool) {
    let orphan_user_id = Uuid::new_v4();

    let err = db::contracts::insert_contract(
        &pool,
        orphan_user_id,
        "Employment Agreement",
        "Full contract text placeholder.",
    )
    .await
    .expect_err("contract for a non-existent user must be rejected");

    assert_db_error_code(&err, "23503", "orphan contract");
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_contract_risk_score_is_rejected(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let err = sqlx::query("UPDATE contracts SET risk_score = 101 WHERE id = $1")
        .bind(contract_id)
        .execute(&pool)
        .await
        .expect_err("risk_score > 100 must be rejected");

    assert_db_error_code(&err, "23514", "risk_score > 100");
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_contract_risk_level_is_rejected(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let err = sqlx::query("UPDATE contracts SET risk_level = 'catastrophic' WHERE id = $1")
        .bind(contract_id)
        .execute(&pool)
        .await
        .expect_err("unknown risk_level must be rejected");

    assert_db_error_code(&err, "23514", "unknown risk_level");
}

#[sqlx::test(migrations = "./migrations")]
async fn obligation_can_be_inserted(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let obligation =
        db::obligations::insert_obligation(&pool, contract_id, "Payment due", "Pay within 30 days")
            .await
            .expect("obligation insert should succeed");

    assert_eq!(obligation.contract_id, contract_id);
    assert_eq!(obligation.status, "pending");
    assert!(obligation.due_date.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn obligation_requires_existing_contract(pool: PgPool) {
    let orphan_contract_id = Uuid::new_v4();

    let err = db::obligations::insert_obligation(
        &pool,
        orphan_contract_id,
        "Payment due",
        "Pay within 30 days",
    )
    .await
    .expect_err("obligation for a non-existent contract must be rejected");

    assert_db_error_code(&err, "23503", "orphan obligation");
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_obligation_status_is_rejected(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;
    let obligation_id =
        db::obligations::insert_obligation(&pool, contract_id, "Payment due", "Pay within 30 days")
            .await
            .unwrap()
            .id;

    let err = sqlx::query("UPDATE contract_obligations SET status = 'paused' WHERE id = $1")
        .bind(obligation_id)
        .execute(&pool)
        .await
        .expect_err("unknown obligation status must be rejected");

    assert_db_error_code(&err, "23514", "unknown obligation status");
}

#[sqlx::test(migrations = "./migrations")]
async fn risk_can_be_inserted(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let risk = db::risks::insert_risk(
        &pool,
        contract_id,
        "Automatic renewal",
        "The contract renews automatically unless cancelled 90 days prior.",
        "high",
        75,
        "Clause 4.2: This agreement shall renew automatically...",
    )
    .await
    .expect("risk insert should succeed");

    assert_eq!(risk.risk_level, "high");
    assert_eq!(risk.risk_score, 75);
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_risk_score_is_rejected(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let err = db::risks::insert_risk(
        &pool,
        contract_id,
        "Negative score",
        "Should be rejected.",
        "low",
        -1,
        "Evidence.",
    )
    .await
    .expect_err("negative risk_score must be rejected");

    assert_db_error_code(&err, "23514", "negative risk_score");
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_risk_level_is_rejected(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let err = db::risks::insert_risk(
        &pool,
        contract_id,
        "Bad level",
        "Should be rejected.",
        "catastrophic",
        50,
        "Evidence.",
    )
    .await
    .expect_err("unknown risk_level must be rejected");

    assert_db_error_code(&err, "23514", "unknown risk_level");
}

#[sqlx::test(migrations = "./migrations")]
async fn reminder_can_be_inserted(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let reminder = db::reminders::insert_reminder(
        &pool,
        contract_id,
        None,
        Utc::now() + Duration::days(7),
        "7_days_before",
        "telegram",
    )
    .await
    .expect("reminder insert should succeed");

    assert_eq!(reminder.status, "pending");
    assert_eq!(reminder.attempts, 0);
    assert!(reminder.sent_at.is_none());
    assert!(reminder.obligation_id.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn reminder_requires_existing_contract(pool: PgPool) {
    let orphan_contract_id = Uuid::new_v4();

    let err = db::reminders::insert_reminder(
        &pool,
        orphan_contract_id,
        None,
        Utc::now() + Duration::days(7),
        "7_days_before",
        "telegram",
    )
    .await
    .expect_err("reminder for a non-existent contract must be rejected");

    assert_db_error_code(&err, "23503", "orphan reminder");
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_reminder_status_is_rejected(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;
    let reminder_id = db::reminders::insert_reminder(
        &pool,
        contract_id,
        None,
        Utc::now() + Duration::days(7),
        "7_days_before",
        "telegram",
    )
    .await
    .unwrap()
    .id;

    let err = sqlx::query("UPDATE reminders SET status = 'queued' WHERE id = $1")
        .bind(reminder_id)
        .execute(&pool)
        .await
        .expect_err("unknown reminder status must be rejected");

    assert_db_error_code(&err, "23514", "unknown reminder status");
}

#[sqlx::test(migrations = "./migrations")]
async fn reminder_obligation_must_belong_to_same_contract(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_a = seed_contract(&pool, user_id).await;
    let contract_b = seed_contract(&pool, user_id).await;

    let obligation_on_b =
        db::obligations::insert_obligation(&pool, contract_b, "Payment due", "Pay within 30 days")
            .await
            .unwrap()
            .id;

    let err = db::reminders::insert_reminder(
        &pool,
        contract_a,
        Some(obligation_on_b),
        Utc::now() + Duration::days(7),
        "7_days_before",
        "telegram",
    )
    .await
    .expect_err("cross-contract obligation link must be rejected");

    assert_db_error_code(&err, "23503", "cross-contract obligation link");
}

#[sqlx::test(migrations = "./migrations")]
async fn due_reminder_query_locates_pending_reminders(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let due_pending = db::reminders::insert_reminder(
        &pool,
        contract_id,
        None,
        Utc::now() - Duration::hours(1),
        "on_deadline",
        "telegram",
    )
    .await
    .unwrap();

    let _future_pending = db::reminders::insert_reminder(
        &pool,
        contract_id,
        None,
        Utc::now() + Duration::days(1),
        "1_day_before",
        "telegram",
    )
    .await
    .unwrap();

    let sent = db::reminders::insert_reminder(
        &pool,
        contract_id,
        None,
        Utc::now() - Duration::hours(2),
        "7_days_before",
        "telegram",
    )
    .await
    .unwrap();
    sqlx::query("UPDATE reminders SET status = 'sent', sent_at = NOW() WHERE id = $1")
        .bind(sent.id)
        .execute(&pool)
        .await
        .unwrap();

    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM reminders \
         WHERE status = 'pending' AND reminder_date <= NOW() \
         ORDER BY reminder_date ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("worker query should succeed");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, due_pending.id);
}

#[sqlx::test(migrations = "./migrations")]
async fn audit_log_can_be_stored(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;

    let entry = db::audit::insert_audit(
        &pool,
        Some(user_id),
        "contract_created",
        "contract",
        Some(contract_id),
        json!({"source": "api"}),
    )
    .await
    .expect("audit insert should succeed");

    assert_eq!(entry.action, "contract_created");
    assert_eq!(entry.entity_type, "contract");
    assert_eq!(entry.user_id, Some(user_id));
    assert_eq!(entry.metadata["source"], "api");
}

#[sqlx::test(migrations = "./migrations")]
async fn audit_log_accepts_system_events_without_user(pool: PgPool) {
    let entry = db::audit::insert_audit(
        &pool,
        None,
        "notification_failed",
        "reminder",
        None,
        json!({"reason": "channel_unavailable"}),
    )
    .await
    .expect("system-generated audit insert should succeed");

    assert!(entry.user_id.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_user_cascades_to_contracts_and_below(pool: PgPool) {
    let user_id = seed_user(&pool).await;
    let contract_id = seed_contract(&pool, user_id).await;
    let obligation_id =
        db::obligations::insert_obligation(&pool, contract_id, "Payment due", "Pay within 30 days")
            .await
            .unwrap()
            .id;
    db::risks::insert_risk(
        &pool,
        contract_id,
        "Automatic renewal",
        "Renews unless cancelled.",
        "high",
        75,
        "Clause 4.2",
    )
    .await
    .unwrap();
    db::reminders::insert_reminder(
        &pool,
        contract_id,
        Some(obligation_id),
        Utc::now() + Duration::days(7),
        "7_days_before",
        "telegram",
    )
    .await
    .unwrap();

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("user delete should succeed");

    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT count(*) FROM contracts WHERE user_id = $1), \
            (SELECT count(*) FROM contract_obligations WHERE contract_id = $2), \
            (SELECT count(*) FROM contract_risks WHERE contract_id = $2), \
            (SELECT count(*) FROM reminders WHERE contract_id = $2)",
    )
    .bind(user_id)
    .bind(contract_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(counts, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_user_nullifies_audit_user_id_but_keeps_row(pool: PgPool) {
    let user_id = seed_user(&pool).await;

    db::audit::insert_audit(
        &pool,
        Some(user_id),
        "contract_created",
        "contract",
        None,
        json!({}),
    )
    .await
    .unwrap();

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("user delete should succeed");

    let rows: Vec<(Option<Uuid>,)> =
        sqlx::query_as("SELECT user_id FROM audit_logs WHERE user_id IS NULL")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(rows.len(), 1, "audit row must survive with NULL user_id");
}
