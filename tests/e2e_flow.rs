//! End-to-end integration tests for the full LexHack workflow.
//!
//! Complete chain exercised:
//!
//!     register → login → create contract → AI analysis
//!       → risks/obligations → generate reminders → configure channel
//!       → worker claims and dispatches → audit trail
//!
//! All tests run against a real PostgreSQL via `#[sqlx::test]` and use
//! two test doubles:
//!
//! * `MockProvider` — in-process `AiProvider` returning deterministic
//!   structured output. No external AI service is contacted.
//! * A dead webhook URL (port 1) — the worker will fail to deliver with
//!   a transient error and schedule a retry. That is the expected path
//!   and is what we assert.
//!
//! `WEBHOOK_ALLOW_LOOPBACK=true` is set in the test config **only** so
//! the worker can attempt a loopback connection. It must never be
//! enabled in production.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;

use lexhack_backend::ai::schema::{AiAnalysis, AiContractDates, AiObligation, AiRisk};
use lexhack_backend::ai::{AiError, AiProvider};
use lexhack_backend::api;
use lexhack_backend::config::Config;
use lexhack_backend::state::AppState;
use lexhack_backend::worker::poller::ChannelRegistry;

const TEST_IP: &str = "127.0.0.1:5000";
const VALID_PASSWORD: &str = "correct horse battery staple";

// =========================================================================
// Test configuration
// =========================================================================

fn test_config() -> Config {
    let pairs: HashMap<&str, &str> = [
        ("DATABASE_URL", "postgres://test/test@localhost/test"),
        ("SESSION_LIFETIME_SECONDS", "3600"),
        ("ARGON2_M_COST", "19456"),
        ("ARGON2_T_COST", "2"),
        ("ARGON2_P_COST", "1"),
        ("RATE_LIMIT_LOGIN_PER_MINUTE", "1000"),
        ("RATE_LIMIT_REGISTER_PER_HOUR", "1000"),
        ("RATE_LIMIT_ANALYZE_PER_HOUR", "1000"),
        ("RATE_LIMIT_CHANNEL_TEST_PER_MINUTE", "1000"),
        ("RATE_LIMIT_CHANNEL_MUTATION_PER_HOUR", "1000"),
        ("AI_MAX_RETRIES", "0"),
        ("AI_MAX_INPUT_CHARS", "100000"),
        (
            "NOTIFICATION_SECRET_KEY",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        ),
        ("WORKER_ENABLED", "true"),
        ("WORKER_POLL_INTERVAL_SECONDS", "1"),
        ("WORKER_BATCH_SIZE", "10"),
        ("WORKER_MAX_CONCURRENCY", "4"),
        ("WORKER_CLAIM_TIMEOUT_SECONDS", "60"),
        ("WORKER_SHUTDOWN_TIMEOUT_SECONDS", "5"),
        ("NOTIFICATION_MAX_RETRIES", "2"),
        ("NOTIFICATION_REQUEST_TIMEOUT_SECONDS", "3"),
        ("WEBHOOK_ALLOW_LOOPBACK", "true"),
    ]
    .into_iter()
    .collect();
    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string()))
        .expect("test config should be valid")
}

// =========================================================================
// Mock AI provider — returns a future due_date so reminders get created
// =========================================================================

#[derive(Debug)]
struct MockProvider;

#[async_trait]
impl AiProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn model(&self) -> &str {
        "mock-model-1"
    }
    async fn analyze_contract(&self, _raw_text: &str) -> Result<AiAnalysis, AiError> {
        // 30 days from now — guarantees the reminder generator produces
        // the full 7/3/1/0 schedule.
        let due_date = (chrono::Utc::now().date_naive() + chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();

        Ok(AiAnalysis {
            contract_dates: AiContractDates {
                start_date: Some("2026-01-01".to_string()),
                end_date: Some("2026-12-31".to_string()),
            },
            risks: vec![AiRisk {
                title: "Automatic renewal".to_string(),
                description: "Contract renews automatically.".to_string(),
                risk_level: "high".to_string(),
                risk_score: 75,
                evidence: "Clause 4.2: This agreement shall renew automatically".to_string(),
            }],
            obligations: vec![AiObligation {
                title: "Payment due".to_string(),
                description: "Pay within 30 days.".to_string(),
                due_date: Some(due_date),
                responsible_party: Some("Client".to_string()),
                status: "pending".to_string(),
                risk_level: Some("low".to_string()),
            }],
        })
    }
}

// =========================================================================
// Environment helpers
// =========================================================================

fn build_state(pool: PgPool) -> AppState {
    let provider: Arc<dyn AiProvider> = Arc::new(MockProvider);
    let channels = Arc::new(ChannelRegistry::test_stub());
    AppState::with_provider(test_config(), pool, provider, channels)
}

fn app(pool: PgPool) -> Router {
    api::build_router(build_state(pool))
}

fn request(method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let body = match body {
        Some(v) => Body::from(v.to_string()),
        None => Body::empty(),
    };
    let mut req = b.body(body).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(TEST_IP.parse::<SocketAddr>().unwrap()));
    req
}

async fn json_body(r: axum::response::Response) -> Value {
    let b = to_bytes(r.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&b).unwrap_or(Value::Null)
}

async fn register(app: &Router, email: &str) -> String {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": email,
                "password": VALID_PASSWORD,
                "full_name": "E2E User",
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED, "register should succeed");
    json_body(r).await["token"].as_str().unwrap().to_string()
}

async fn create_contract(app: &Router, token: &str) -> Uuid {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({
                "title": "E2E Employment Agreement",
                "raw_text": "This fictional agreement is used only for testing. \
                              Clause 4.2: This agreement shall renew automatically \
                              for successive 12-month periods."
            })),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED, "create contract");
    Uuid::parse_str(json_body(r).await["id"].as_str().unwrap()).unwrap()
}

async fn analyze(app: &Router, token: &str, id: Uuid) -> Value {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{id}/analyze"),
            None,
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK, "analyze should succeed");
    json_body(r).await
}

async fn get_analysis(app: &Router, token: &str, id: Uuid) -> Value {
    let r = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{id}/analysis"),
            None,
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK, "get analysis");
    json_body(r).await
}

async fn generate_reminders(app: &Router, token: &str, id: Uuid) -> Value {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{id}/reminders/generate"),
            None,
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK, "generate reminders");
    json_body(r).await
}

async fn create_webhook_channel(app: &Router, token: &str, url: &str) -> Uuid {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "webhook",
                "name": "E2E webhook",
                "url": url,
            })),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED, "create channel");
    Uuid::parse_str(json_body(r).await["id"].as_str().unwrap()).unwrap()
}

/// Spawns the worker, waits `run_secs`, then cancels it cleanly.
async fn run_worker_briefly(pool: &PgPool, run_secs: u64) {
    let state = build_state(pool.clone());
    let secret_key = Arc::new(state.secret_key().clone());
    let cancel = CancellationToken::new();
    let handle = lexhack_backend::worker::spawn(
        test_config(),
        pool.clone(),
        state.channels(),
        secret_key,
        cancel.clone(),
    );
    tokio::time::sleep(Duration::from_secs(run_secs)).await;
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(10), handle).await;
}

// =========================================================================
// Test 1 — full workflow register → audit
// =========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn full_workflow_register_to_audit(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "e2e@example.com").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'e2e@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Create contract.
    let contract_id = create_contract(&app, &token).await;

    // Analyze.
    let analysis = analyze(&app, &token, contract_id).await;
    assert_eq!(analysis["analysis_status"], "completed");
    assert_eq!(analysis["risks"].as_array().unwrap().len(), 1);
    assert_eq!(analysis["obligations"].as_array().unwrap().len(), 1);

    // Read-only fetch returns the same result.
    let read_back = get_analysis(&app, &token, contract_id).await;
    assert_eq!(read_back["analysis_status"], "completed");
    assert_eq!(read_back["risks"].as_array().unwrap().len(), 1);

    // DB confirms persistence.
    let risk_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contract_risks WHERE contract_id = $1")
            .bind(contract_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(risk_count, 1);
    let obligation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contract_obligations WHERE contract_id = $1")
            .bind(contract_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(obligation_count, 1);

    // Generate reminders.
    let gen = generate_reminders(&app, &token, contract_id).await;
    assert!(
        gen["created"].as_u64().unwrap() >= 4,
        "at least 4 reminders"
    );

    let reminder_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reminders WHERE contract_id = $1")
            .bind(contract_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(reminder_count >= 4);

    // Create webhook channel pointing at an unreachable destination.
    // The worker will attempt delivery and record a transient failure.
    let _channel_id = create_webhook_channel(&app, &token, "https://127.0.0.1:1/e2e").await;

    // Run worker — it should claim due reminders and attempt delivery.
    // The very next reminder is at least `30 - 7 = 23` days away, so
    // nothing is due yet. To exercise claim + dispatch we instead push
    // one pending reminder's date into the past directly.
    sqlx::query(
        "UPDATE reminders SET reminder_date = NOW() - INTERVAL '1 minute' \
         WHERE contract_id = $1 AND status = 'pending' LIMIT 1",
    )
    .bind(contract_id)
    .execute(&pool)
    .await
    .ok(); // `.LIMIT` in UPDATE requires a subquery; fall back below.

    // PostgreSQL does not support LIMIT in UPDATE without a subquery.
    // Do it explicitly.
    sqlx::query(
        "UPDATE reminders SET reminder_date = NOW() - INTERVAL '1 minute' \
         WHERE id = (SELECT id FROM reminders \
                     WHERE contract_id = $1 AND status = 'pending' \
                     ORDER BY reminder_date ASC LIMIT 1)",
    )
    .bind(contract_id)
    .execute(&pool)
    .await
    .unwrap();

    run_worker_briefly(&pool, 4).await;

    // After the worker, at least one reminder should have been *touched*:
    // either it is now `pending` with attempts >= 1 (retry scheduled),
    // or `failed` (retries exhausted). Either proves the worker ran.
    let touched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reminders \
         WHERE contract_id = $1 AND attempts >= 1",
    )
    .bind(contract_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        touched >= 1,
        "worker must have claimed and attempted at least one reminder"
    );

    // Audit trail is present.
    let actions: Vec<String> =
        sqlx::query_scalar("SELECT action FROM audit_logs WHERE user_id = $1 ORDER BY created_at")
            .bind(user_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(actions.iter().any(|a| a == "contract_created"));
    assert!(actions.iter().any(|a| a == "contract_analyzed"));
    assert!(actions
        .iter()
        .any(|a| a == "contract_reminders_regenerated"));
}

// =========================================================================
// Test 2 — stale analysis protection across content change
// =========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn stale_analysis_is_invalidated_by_content_change(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "stale@example.com").await;
    let contract_id = create_contract(&app, &token).await;

    analyze(&app, &token, contract_id).await;

    let (status_before, version_before): (String, i32) =
        sqlx::query_as("SELECT analysis_status, content_version FROM contracts WHERE id = $1")
            .bind(contract_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status_before, "completed");
    assert_eq!(version_before, 1);

    // Edit the contract — should bump version and reset status.
    let r = app
        .clone()
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{contract_id}"),
            Some(json!({ "raw_text": "Completely different content." })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);

    let (status_after, version_after): (String, i32) =
        sqlx::query_as("SELECT analysis_status, content_version FROM contracts WHERE id = $1")
            .bind(contract_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status_after, "not_analyzed");
    assert_eq!(version_after, 2);
}

// =========================================================================
// Test 3 — custom reminder creation and cancellation
// =========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn custom_reminder_lifecycle(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "custom@example.com").await;
    let contract_id = create_contract(&app, &token).await;
    analyze(&app, &token, contract_id).await;

    // Create a channel first — reminders must point at an enabled one
    // when dispatched, and the custom reminder form requires it.
    let _ = create_webhook_channel(&app, &token, "https://127.0.0.1:1/custom").await;

    // Create the custom reminder.
    let when = chrono::Utc::now() + chrono::Duration::days(3);
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{contract_id}/reminders"),
            Some(json!({
                "reminder_date": when,
                "channel_type": "webhook",
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let body = json_body(r).await;
    assert_eq!(body["reminder_source"], "custom");
    assert_eq!(body["status"], "pending");
    let reminder_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();

    // Cancel it.
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{reminder_id}/cancel"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(json_body(r).await["status"], "cancelled");

    // Cannot cancel twice.
    let r = app
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{reminder_id}/cancel"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

// =========================================================================
// Test 4 — cross-user isolation through the full workflow
// =========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn cross_user_isolation_end_to_end(pool: PgPool) {
    let app = app(pool.clone());
    let alice = register(&app, "alice-e2e@example.com").await;
    let bob = register(&app, "bob-e2e@example.com").await;

    let alice_contract = create_contract(&app, &alice).await;
    analyze(&app, &alice, alice_contract).await;
    let _ = create_webhook_channel(&app, &alice, "https://127.0.0.1:1/alice").await;

    // Bob cannot read Alice's contract.
    let r = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{alice_contract}"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);

    // Bob cannot read Alice's analysis.
    let r = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{alice_contract}/analysis"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);

    // Bob cannot generate reminders on Alice's contract.
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{alice_contract}/reminders/generate"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);

    // Bob cannot list Alice's channels.
    let r = app
        .oneshot(request(
            "GET",
            "/api/notification-channels",
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["total"], 0, "Bob must not see Alice's channels");
}
