//! Integration tests for the PART 07 background reminder worker.
//!
//! All tests require a live PostgreSQL instance via `#[sqlx::test]`.
//! They exercise the SQL claim / recovery / outcome semantics directly
//! (not the poller loop, which is timing-dependent and would slow the
//! suite). The delivery abstraction is a mock channel defined below;
//! no real notification service is contacted.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use lexhack_backend::notifications::channel::{DeliveryError, Notification, NotificationChannel};
use lexhack_backend::worker::{claim, outcome, recovery};

// -------------------------------------------------------------------------
// Fixtures
// -------------------------------------------------------------------------

async fn seed_user(pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, password_hash, full_name) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("w@example.com")
    .bind("$argon2id$v=19$m=19456,t=2,p=1$placeholder$placeholder")
    .bind("Worker")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_contract(pool: &PgPool, user_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO contracts (user_id, title, raw_text) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(user_id)
    .bind("Contract")
    .bind("body")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_reminder(
    pool: &PgPool,
    contract_id: Uuid,
    when: chrono::DateTime<Utc>,
    status: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO reminders \
             (contract_id, reminder_date, reminder_type, channel_type, status) \
         VALUES ($1, $2, 'on_deadline', 'telegram', $3) \
         RETURNING id",
    )
    .bind(contract_id)
    .bind(when)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap()
}

// -------------------------------------------------------------------------
// Mock channel
// -------------------------------------------------------------------------

#[derive(Debug)]
struct MockChannel {
    result: Result<(), DeliveryError>,
    calls: Arc<AtomicUsize>,
}

impl MockChannel {
    fn new(result: Result<(), DeliveryError>) -> Self {
        Self {
            result,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl NotificationChannel for MockChannel {
    fn name(&self) -> &'static str {
        "mock"
    }
    async fn send(&self, _n: &Notification) -> Result<(), DeliveryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone()
    }
}

// -------------------------------------------------------------------------
// Claim
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn claim_picks_up_due_pending_reminder(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "pending").await;

    let claimed = claim::claim_due(&pool, "worker-1", 10).await.unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, rid);

    let status: String = sqlx::query_scalar("SELECT status FROM reminders WHERE id = $1")
        .bind(rid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "processing");

    let claimed_by: Option<String> =
        sqlx::query_scalar("SELECT claimed_by FROM reminders WHERE id = $1")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(claimed_by.as_deref(), Some("worker-1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_ignores_future_reminder(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    seed_reminder(&pool, cid, Utc::now() + Duration::days(1), "pending").await;

    let claimed = claim::claim_due(&pool, "worker-1", 10).await.unwrap();
    assert!(claimed.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_ignores_cancelled_reminder(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "cancelled").await;

    let claimed = claim::claim_due(&pool, "worker-1", 10).await.unwrap();
    assert!(claimed.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_ignores_sent_reminder(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "sent").await;

    let claimed = claim::claim_due(&pool, "worker-1", 10).await.unwrap();
    assert!(claimed.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_ignores_reminder_with_future_next_attempt(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "pending").await;
    sqlx::query(
        "UPDATE reminders SET next_attempt_at = NOW() + interval '10 minutes' WHERE id = $1",
    )
    .bind(rid)
    .execute(&pool)
    .await
    .unwrap();

    let claimed = claim::claim_due(&pool, "worker-1", 10).await.unwrap();
    assert!(claimed.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_respects_batch_size(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    for i in 0..5 {
        seed_reminder(
            &pool,
            cid,
            Utc::now() - Duration::hours(1) + Duration::seconds(i),
            "pending",
        )
        .await;
    }

    let claimed = claim::claim_due(&pool, "worker-1", 3).await.unwrap();
    assert_eq!(claimed.len(), 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_order_is_oldest_first(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let old = seed_reminder(&pool, cid, Utc::now() - Duration::hours(2), "pending").await;
    let recent = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "pending").await;

    let claimed = claim::claim_due(&pool, "worker-1", 10).await.unwrap();
    assert_eq!(claimed[0].id, old);
    assert_eq!(claimed[1].id, recent);
}

// -------------------------------------------------------------------------
// Concurrency
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_workers_do_not_double_claim(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "pending").await;

    let p1 = pool.clone();
    let p2 = pool.clone();
    let (a, b) = tokio::join!(
        tokio::spawn(async move { claim::claim_due(&p1, "w1", 10).await.unwrap() }),
        tokio::spawn(async move { claim::claim_due(&p2, "w2", 10).await.unwrap() }),
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(
        a.len() + b.len(),
        1,
        "exactly one worker must win the claim"
    );
}

// -------------------------------------------------------------------------
// Recovery
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn recovery_resets_stale_processing(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "processing").await;
    sqlx::query("UPDATE reminders SET claimed_at = NOW() - interval '10 minutes', claimed_by = 'w1' WHERE id = $1")
        .bind(rid)
        .execute(&pool)
        .await
        .unwrap();

    let n = recovery::recover_stale_processing(&pool, 300)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let (status, claimed_by): (String, Option<String>) =
        sqlx::query_as("SELECT status, claimed_by FROM reminders WHERE id = $1")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");
    assert!(claimed_by.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn recovery_leaves_fresh_processing_alone(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "processing").await;
    sqlx::query("UPDATE reminders SET claimed_at = NOW(), claimed_by = 'w1' WHERE id = $1")
        .bind(rid)
        .execute(&pool)
        .await
        .unwrap();

    let n = recovery::recover_stale_processing(&pool, 300)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

// -------------------------------------------------------------------------
// Outcome
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn mark_sent_only_when_owned(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "processing").await;
    sqlx::query("UPDATE reminders SET claimed_by = 'w1' WHERE id = $1")
        .bind(rid)
        .execute(&pool)
        .await
        .unwrap();

    // Wrong worker id → no change.
    let ok = outcome::mark_sent(&pool, rid, "w2").await.unwrap();
    assert!(!ok);

    // Correct worker id → success.
    let ok = outcome::mark_sent(&pool, rid, "w1").await.unwrap();
    assert!(ok);

    let (status, attempts, sent_at): (String, i32, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as("SELECT status, attempts, sent_at FROM reminders WHERE id = $1")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "sent");
    assert_eq!(attempts, 1);
    assert!(sent_at.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn retryable_failure_keeps_pending_with_backoff(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "processing").await;
    sqlx::query("UPDATE reminders SET claimed_by = 'w1' WHERE id = $1")
        .bind(rid)
        .execute(&pool)
        .await
        .unwrap();

    let err = DeliveryError::Transient;
    let will_retry = outcome::schedule_retry_or_fail(&pool, rid, "w1", &err, 5)
        .await
        .unwrap();
    assert!(will_retry);

    let (status, attempts, next): (String, i32, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as("SELECT status, attempts, next_attempt_at FROM reminders WHERE id = $1")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");
    assert_eq!(attempts, 1);
    assert!(next.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn permanent_failure_marks_failed(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "processing").await;
    sqlx::query("UPDATE reminders SET claimed_by = 'w1' WHERE id = $1")
        .bind(rid)
        .execute(&pool)
        .await
        .unwrap();

    let err = DeliveryError::NotConfigured;
    let will_retry = outcome::schedule_retry_or_fail(&pool, rid, "w1", &err, 5)
        .await
        .unwrap();
    assert!(!will_retry);

    let (status, last_error): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM reminders WHERE id = $1")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(last_error.as_deref(), Some("channel_not_configured"));
}

#[sqlx::test(migrations = "./migrations")]
async fn retry_limit_exhaustion_fails(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "processing").await;
    // Pretend we've already had max_retries - 1 attempts.
    sqlx::query("UPDATE reminders SET claimed_by = 'w1', attempts = 4 WHERE id = $1")
        .bind(rid)
        .execute(&pool)
        .await
        .unwrap();

    let err = DeliveryError::Timeout;
    let will_retry = outcome::schedule_retry_or_fail(&pool, rid, "w1", &err, 5)
        .await
        .unwrap();
    assert!(!will_retry, "attempt 5 of 5 must fail, not retry");

    let (status, attempts): (String, i32) =
        sqlx::query_as("SELECT status, attempts FROM reminders WHERE id = $1")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(attempts, 5);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancelled_reminder_cannot_be_marked_sent(pool: PgPool) {
    let uid = seed_user(&pool).await;
    let cid = seed_contract(&pool, uid).await;
    let rid = seed_reminder(&pool, cid, Utc::now() - Duration::hours(1), "cancelled").await;

    let ok = outcome::mark_sent(&pool, rid, "w1").await.unwrap();
    assert!(!ok);
}

// -------------------------------------------------------------------------
// Mock channel smoke test
// -------------------------------------------------------------------------

#[tokio::test]
async fn mock_channel_reports_configured_result() {
    let ch = MockChannel::new(Ok(()));
    let n = Notification {
        reminder_id: Uuid::new_v4(),
        contract_id: Uuid::new_v4(),
        obligation_id: None,
        title: "T".to_string(),
        message: "M".to_string(),
        due_date: None,
        reminder_type: "on_deadline".to_string(),
        channel_type: lexhack_backend::reminders::types::ChannelType::Telegram,
    };
    ch.send(&n).await.unwrap();
    assert_eq!(ch.calls.load(Ordering::SeqCst), 1);
}
