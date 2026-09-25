//! Integration tests for the PART 06 reminder engine.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use lexhack_backend::ai::schema::{AiAnalysis, AiContractDates, AiObligation};
use lexhack_backend::ai::{AiError, AiProvider};
use lexhack_backend::api;
use lexhack_backend::config::Config;
use lexhack_backend::state::AppState;
use lexhack_backend::worker::poller::ChannelRegistry;

use async_trait::async_trait;
use std::sync::Arc;

const TEST_IP: &str = "127.0.0.1:5000";
const VALID_PASSWORD: &str = "correct horse battery staple";

#[derive(Debug)]
struct StaticProvider(Result<AiAnalysis, AiError>);

#[async_trait]
impl AiProvider for StaticProvider {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn model(&self) -> &str {
        "mock-model"
    }
    async fn analyze_contract(&self, _: &str) -> Result<AiAnalysis, AiError> {
        self.0.clone()
    }
}

fn analysis_with_obligations(due_dates: Vec<&str>, end_date: Option<&str>) -> AiAnalysis {
    AiAnalysis {
        contract_dates: AiContractDates {
            start_date: None,
            end_date: end_date.map(|s| s.to_string()),
        },
        risks: vec![],
        obligations: due_dates
            .into_iter()
            .enumerate()
            .map(|(i, d)| AiObligation {
                title: format!("Obligation {}", i + 1),
                description: format!("Description {}", i + 1),
                due_date: Some(d.to_string()),
                responsible_party: None,
                status: "pending".to_string(),
                risk_level: None,
            })
            .collect(),
    }
}

fn test_config() -> Config {
    let pairs: HashMap<&str, &str> = [
        ("DATABASE_URL", "postgres://test/test@localhost/test"),
        ("SESSION_LIFETIME_SECONDS", "3600"),
        ("ARGON2_M_COST", "19456"),
        ("ARGON2_T_COST", "2"),
        ("ARGON2_P_COST", "1"),
        ("RATE_LIMIT_LOGIN_PER_MINUTE", "1000"),
        ("RATE_LIMIT_REGISTER_PER_HOUR", "1000"),
        ("AI_MAX_RETRIES", "0"),
        ("AI_MAX_INPUT_CHARS", "100000"),
    ]
    .into_iter()
    .collect();
    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string())).unwrap()
}

fn app(pool: PgPool, analysis: AiAnalysis) -> Router {
    let p: Arc<dyn AiProvider> = Arc::new(StaticProvider(Ok(analysis)));
    api::build_router(AppState::with_provider(
        test_config(),
        pool,
        p,
        std::sync::Arc::new(ChannelRegistry::test_stub()),
    ))
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
            Some(json!({"email": email, "password": VALID_PASSWORD, "full_name": "T"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    json_body(r).await["token"].as_str().unwrap().to_string()
}

async fn create_contract(app: &Router, token: &str) -> Uuid {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "T", "raw_text": "R"})),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    Uuid::parse_str(json_body(r).await["id"].as_str().unwrap()).unwrap()
}

async fn run_analysis(app: &Router, token: &str, cid: Uuid) {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid}/analyze"),
            None,
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

fn future_date(days: i64) -> String {
    (Utc::now().date_naive() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

async fn generate(app: &Router, token: &str, cid: Uuid) -> axum::response::Response {
    app.clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid}/reminders/generate"),
            None,
            Some(token),
        ))
        .await
        .unwrap()
}

async fn list(app: &Router, token: &str, cid: Uuid) -> Value {
    let r = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{cid}/reminders"),
            None,
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    json_body(r).await
}

// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn generate_standard_reminders_for_obligation(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let r = generate(&app, &token, cid).await;
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["created"], 4);
    assert_eq!(b["preserved_custom"], 0);

    let body = list(&app, &token, cid).await;
    assert_eq!(body["total"], 4);
}

#[sqlx::test(migrations = "./migrations")]
async fn generate_contract_level_end_date_reminders(pool: PgPool) {
    let end = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![], Some(&end)));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let b = json_body(generate(&app, &token, cid).await).await;
    assert_eq!(b["created"], 4);

    let list_body = list(&app, &token, cid).await;
    for item in list_body["items"].as_array().unwrap() {
        assert!(item["obligation_id"].is_null());
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn past_deadline_generates_nothing(pool: PgPool) {
    let past = (Utc::now().date_naive() - Duration::days(5))
        .format("%Y-%m-%d")
        .to_string();
    let app = app(pool.clone(), analysis_with_obligations(vec![&past], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let b = json_body(generate(&app, &token, cid).await).await;
    assert_eq!(b["created"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn deadline_tomorrow_gets_one_day_before_and_on_deadline(pool: PgPool) {
    let due = future_date(1);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    // Tomorrow's deadline still has two meaningful offsets: 1_day_before
    // (today) and on_deadline (tomorrow). 7d and 3d are past.
    let b = json_body(generate(&app, &token, cid).await).await;
    assert_eq!(b["created"], 2);

    let l = list(&app, &token, cid).await;
    assert_eq!(l["total"], 2);
    let types: Vec<&str> = l["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["reminder_type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"on_deadline"));
    assert!(types.contains(&"1_day_before"));
}

#[sqlx::test(migrations = "./migrations")]
async fn deadline_in_three_days_three_reminders(pool: PgPool) {
    let due = future_date(3);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let b = json_body(generate(&app, &token, cid).await).await;
    assert_eq!(b["created"], 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn deadline_in_seven_days_four_reminders(pool: PgPool) {
    let due = future_date(7);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let b = json_body(generate(&app, &token, cid).await).await;
    assert_eq!(b["created"], 4);
}

#[sqlx::test(migrations = "./migrations")]
async fn regeneration_is_idempotent(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let _ = generate(&app, &token, cid).await;
    let _ = generate(&app, &token, cid).await;
    let _ = generate(&app, &token, cid).await;

    let l = list(&app, &token, cid).await;
    assert_eq!(l["total"], 4, "idempotent regeneration must not duplicate");
}

#[sqlx::test(migrations = "./migrations")]
async fn custom_reminder_preserved_on_regeneration(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let when = Utc::now() + Duration::days(10);
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid}/reminders"),
            Some(json!({"reminder_date": when, "channel_type": "webhook"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let custom_id = json_body(r).await["id"].as_str().unwrap().to_string();

    let _ = generate(&app, &token, cid).await;
    let l = list(&app, &token, cid).await;
    let found_custom = l["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"].as_str() == Some(&custom_id));
    assert!(found_custom, "custom reminder must survive regeneration");
    assert_eq!(l["total"], 5);
}

#[sqlx::test(migrations = "./migrations")]
async fn custom_reminder_requires_valid_channel(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid}/reminders"),
            Some(json!({"reminder_date": Utc::now() + Duration::days(3), "channel_type": "carrier_pigeon"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn custom_reminder_rejects_foreign_obligation(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let cid_a = create_contract(&app, &alice).await;
    run_analysis(&app, &alice, cid_a).await;

    // Get one of alice's obligation IDs.
    let obligations = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM contract_obligations WHERE contract_id = $1 LIMIT 1",
    )
    .bind(cid_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Bob creates a contract; tries to reference alice's obligation.
    let cid_b = create_contract(&app, &bob).await;

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid_b}/reminders"),
            Some(json!({
                "obligation_id": obligations,
                "reminder_date": Utc::now() + Duration::days(1),
                "channel_type": "telegram"
            })),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_pending_reminder(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;
    let _ = generate(&app, &token, cid).await;

    let l = list(&app, &token, cid).await;
    let rid = l["items"][0]["id"].as_str().unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{rid}/cancel"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["status"], "cancelled");
}

#[sqlx::test(migrations = "./migrations")]
async fn cancelled_reminder_cannot_be_cancelled_again(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;
    let _ = generate(&app, &token, cid).await;

    let l = list(&app, &token, cid).await;
    let rid = l["items"][0]["id"].as_str().unwrap();

    let _ = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{rid}/cancel"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{rid}/cancel"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn sent_reminder_cannot_be_cancelled(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;
    let _ = generate(&app, &token, cid).await;

    // Force one into 'sent'.
    sqlx::query("UPDATE reminders SET status = 'sent', sent_at = NOW() WHERE contract_id = $1")
        .bind(cid)
        .execute(&pool)
        .await
        .unwrap();

    let l = list(&app, &token, cid).await;
    let rid = l["items"][0]["id"].as_str().unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{rid}/cancel"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn cannot_cancel_another_users_reminder(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let cid = create_contract(&app, &alice).await;
    run_analysis(&app, &alice, cid).await;
    let _ = generate(&app, &alice, cid).await;

    let l = list(&app, &alice, cid).await;
    let rid = l["items"][0]["id"].as_str().unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/reminders/{rid}/cancel"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn cannot_list_another_users_reminders(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let cid = create_contract(&app, &alice).await;
    run_analysis(&app, &alice, cid).await;
    let _ = generate(&app, &alice, cid).await;

    let r = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{cid}/reminders"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn cannot_generate_for_another_users_contract(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let cid = create_contract(&app, &alice).await;
    run_analysis(&app, &alice, cid).await;

    let r = generate(&app, &bob, cid).await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn generate_requires_completed_analysis(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    // No analysis run.

    let r = generate(&app, &token, cid).await;
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn generate_rejects_stale_analysis(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    // Simulate a raw_text edit: content_version increases but analysis
    // is now stale.
    sqlx::query(
        "UPDATE contracts SET raw_text = 'updated', content_version = content_version + 1, \
         analysis_status = 'not_analyzed' WHERE id = $1",
    )
    .bind(cid)
    .execute(&pool)
    .await
    .unwrap();

    let r = generate(&app, &token, cid).await;
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_obligation_due_dates_no_extra_reminders(pool: PgPool) {
    let due = future_date(30);
    let app = app(
        pool.clone(),
        analysis_with_obligations(vec![&due, &due], None),
    );
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;

    let b = json_body(generate(&app, &token, cid).await).await;
    // Two obligations, each producing 4 reminders -> 8 distinct rows
    // because obligation_id differs. The uniqueness key includes
    // obligation_id, so this is fine.
    assert_eq!(b["created"], 8);
}

#[sqlx::test(migrations = "./migrations")]
async fn reminder_generation_writes_audit(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;
    run_analysis(&app, &token, cid).await;
    let _ = generate(&app, &token, cid).await;

    let action: String = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE entity_id = $1 AND action = 'contract_reminders_regenerated'",
    )
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(action, "contract_reminders_regenerated");
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_contract_uuid_rejected(pool: PgPool) {
    let due = future_date(30);
    let app = app(pool.clone(), analysis_with_obligations(vec![&due], None));
    let token = register(&app, "a@example.com").await;

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/contracts/not-a-uuid/reminders/generate",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}
