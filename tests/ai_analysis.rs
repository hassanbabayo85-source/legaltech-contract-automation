//! Integration tests for the PART 05 AI analysis workflow.
//! Uses MockProvider; never calls a real AI service.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
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

#[derive(Debug)]
struct MockProvider {
    responses: Mutex<Vec<Result<AiAnalysis, AiError>>>,
    index: AtomicUsize,
    call_count: AtomicUsize,
}

impl MockProvider {
    fn one(r: Result<AiAnalysis, AiError>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(vec![r]),
            index: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
        })
    }
    fn script(rs: Vec<Result<AiAnalysis, AiError>>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(rs),
            index: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
        })
    }
    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AiProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn model(&self) -> &str {
        "mock-model-1"
    }
    async fn analyze_contract(&self, _: &str) -> Result<AiAnalysis, AiError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let idx = self.index.fetch_add(1, Ordering::SeqCst);
        let rs = self.responses.lock().unwrap();
        rs.get(idx)
            .or_else(|| rs.last())
            .cloned()
            .unwrap_or(Err(AiError::Other))
    }
}

fn valid_analysis() -> AiAnalysis {
    AiAnalysis {
        contract_dates: AiContractDates {
            start_date: Some("2026-01-01".to_string()),
            end_date: Some("2026-12-31".to_string()),
        },
        risks: vec![AiRisk {
            title: "Automatic renewal".to_string(),
            description: "Renews automatically".to_string(),
            risk_level: "high".to_string(),
            risk_score: 75,
            evidence: "Clause 4.2: This agreement shall renew automatically".to_string(),
        }],
        obligations: vec![AiObligation {
            title: "Payment due".to_string(),
            description: "Pay within 30 days".to_string(),
            due_date: Some("2026-06-30".to_string()),
            responsible_party: Some("Client".to_string()),
            status: "pending".to_string(),
            risk_level: Some("low".to_string()),
        }],
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
        ("AI_MAX_RETRIES", "1"),
        ("AI_MAX_INPUT_CHARS", "100000"),
    ]
    .into_iter()
    .collect();
    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string())).unwrap()
}

fn app(pool: PgPool, provider: Arc<dyn AiProvider>) -> Router {
    api::build_router(AppState::with_provider(
        test_config(),
        pool,
        provider,
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
    // The raw_text must contain the exact evidence used by
    // `valid_analysis()` so that `verify_risk_evidence()` accepts it.
    let raw_text = "The parties agree to a 2026 term. \
                    Clause 4.2: This agreement shall renew automatically \
                    for successive 12-month periods unless either party \
                    provides 60 days written notice of non-renewal. \
                    Payment is due within 30 days of invoice receipt.";
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "T", "raw_text": raw_text})),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    Uuid::parse_str(json_body(r).await["id"].as_str().unwrap()).unwrap()
}

async fn analyze(app: &Router, token: &str, cid: Uuid) -> axum::response::Response {
    app.clone()
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid}/analyze"),
            None,
            Some(token),
        ))
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_success_persists_risks_and_obligations(pool: PgPool) {
    let p = MockProvider::one(Ok(valid_analysis()));
    let app = app(pool.clone(), p.clone());
    let token = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &token).await;

    let r = analyze(&app, &token, cid).await;
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["analysis_status"], "completed");
    assert_eq!(b["analyzed_content_version"], 1);
    assert_eq!(b["analysis_provider"], "mock");
    assert_eq!(b["start_date"], "2026-01-01");
    assert_eq!(b["end_date"], "2026-12-31");
    assert_eq!(b["risk_score"], 75);
    assert_eq!(b["risks"].as_array().unwrap().len(), 1);
    assert_eq!(b["obligations"].as_array().unwrap().len(), 1);
    assert_eq!(p.calls(), 1);

    let risks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contract_risks WHERE contract_id=$1")
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
    let obs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contract_obligations WHERE contract_id=$1")
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(risks, 1);
    assert_eq!(obs, 1);

    let m: Value = sqlx::query_scalar(
        "SELECT metadata FROM audit_logs WHERE entity_id=$1 AND action='contract_analyzed'",
    )
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(m["content_version"], 1);
    assert!(!m.to_string().contains("Clause 4.2"));
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_invalid_risk_score(pool: PgPool) {
    let mut a = valid_analysis();
    a.risks[0].risk_score = 150;
    let p = MockProvider::one(Ok(a));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let r = analyze(&app, &t, cid).await;
    assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    let s: String = sqlx::query_scalar("SELECT analysis_status FROM contracts WHERE id=$1")
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(s, "failed");
    let e: Option<String> = sqlx::query_scalar("SELECT analysis_error FROM contracts WHERE id=$1")
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(e.as_deref(), Some("invalid_ai_output"));
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_invalid_risk_level(pool: PgPool) {
    let mut a = valid_analysis();
    a.risks[0].risk_level = "catastrophic".to_string();
    let p = MockProvider::one(Ok(a));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    assert_eq!(
        analyze(&app, &t, cid).await.status(),
        StatusCode::BAD_GATEWAY
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_invalid_status(pool: PgPool) {
    let mut a = valid_analysis();
    a.obligations[0].status = "snoozed".to_string();
    let p = MockProvider::one(Ok(a));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    assert_eq!(
        analyze(&app, &t, cid).await.status(),
        StatusCode::BAD_GATEWAY
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_invalid_date(pool: PgPool) {
    let mut a = valid_analysis();
    a.contract_dates.start_date = Some("01/01/2026".to_string());
    let p = MockProvider::one(Ok(a));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    assert_eq!(
        analyze(&app, &t, cid).await.status(),
        StatusCode::BAD_GATEWAY
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_handles_missing_dates_as_null(pool: PgPool) {
    let mut a = valid_analysis();
    a.contract_dates.start_date = None;
    a.contract_dates.end_date = None;
    let p = MockProvider::one(Ok(a));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let r = analyze(&app, &t, cid).await;
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert!(b["start_date"].is_null());
    assert!(b["end_date"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_handles_missing_responsible_party(pool: PgPool) {
    let mut a = valid_analysis();
    a.obligations[0].responsible_party = None;
    let p = MockProvider::one(Ok(a));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let r = analyze(&app, &t, cid).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(json_body(r).await["obligations"][0]["responsible_party"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_handles_timeout(pool: PgPool) {
    let p = MockProvider::one(Err(AiError::Timeout));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let r = analyze(&app, &t, cid).await;
    assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    let (s, e): (String, Option<String>) =
        sqlx::query_as("SELECT analysis_status, analysis_error FROM contracts WHERE id=$1")
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(s, "failed");
    assert_eq!(e.as_deref(), Some("timeout"));
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_does_not_retry_permanent(pool: PgPool) {
    let p = MockProvider::one(Err(AiError::Permanent { status: 400 }));
    let app = app(pool.clone(), p.clone());
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let _ = analyze(&app, &t, cid).await;
    assert_eq!(p.calls(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_retries_transient(pool: PgPool) {
    let p = MockProvider::script(vec![Err(AiError::Transient), Ok(valid_analysis())]);
    let app = app(pool.clone(), p.clone());
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let r = analyze(&app, &t, cid).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(p.calls(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_not_configured(pool: PgPool) {
    let p = MockProvider::one(Err(AiError::NotConfigured));
    let app = app(pool.clone(), p.clone());
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let r = analyze(&app, &t, cid).await;
    assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(p.calls(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_requires_authentication(pool: PgPool) {
    let p = MockProvider::one(Ok(valid_analysis()));
    let app = app(pool, p);
    let cid = Uuid::new_v4();
    let r = app
        .oneshot(request(
            "POST",
            &format!("/api/contracts/{cid}/analyze"),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_foreign_contract(pool: PgPool) {
    let p = MockProvider::one(Ok(valid_analysis()));
    let app = app(pool, p.clone());
    let alice = register(&app, "alice@example.com").await;
    let bob = register(&app, "bob@example.com").await;
    let cid = create_contract(&app, &alice).await;
    let r = analyze(&app, &bob, cid).await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    assert_eq!(p.calls(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_nonexistent_returns_404(pool: PgPool) {
    let p = MockProvider::one(Ok(valid_analysis()));
    let app = app(pool, p);
    let t = register(&app, "a@example.com").await;
    assert_eq!(
        analyze(&app, &t, Uuid::new_v4()).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_rejects_malformed_uuid(pool: PgPool) {
    let p = MockProvider::one(Ok(valid_analysis()));
    let app = app(pool, p);
    let t = register(&app, "a@example.com").await;
    let r = app
        .oneshot(request(
            "POST",
            "/api/contracts/not-a-uuid/analyze",
            None,
            Some(&t),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn analyze_repeated_does_not_duplicate(pool: PgPool) {
    let p = MockProvider::one(Ok(valid_analysis()));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    assert_eq!(analyze(&app, &t, cid).await.status(), StatusCode::OK);
    assert_eq!(analyze(&app, &t, cid).await.status(), StatusCode::OK);
    let r: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contract_risks WHERE contract_id=$1")
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
    let o: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contract_obligations WHERE contract_id=$1")
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(r, 1);
    assert_eq!(o, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_analysis_does_not_complete(pool: PgPool) {
    let p = MockProvider::one(Err(AiError::Permanent { status: 400 }));
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    let _ = analyze(&app, &t, cid).await;
    let s: String = sqlx::query_scalar("SELECT analysis_status FROM contracts WHERE id=$1")
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(s, "failed");
    let at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT analyzed_at FROM contracts WHERE id=$1")
            .bind(cid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(at.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_then_retry_can_succeed(pool: PgPool) {
    let p = MockProvider::script(vec![
        Err(AiError::Permanent { status: 400 }),
        Ok(valid_analysis()),
    ]);
    let app = app(pool.clone(), p);
    let t = register(&app, "a@example.com").await;
    let cid = create_contract(&app, &t).await;
    assert_eq!(
        analyze(&app, &t, cid).await.status(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(analyze(&app, &t, cid).await.status(), StatusCode::OK);
    let s: String = sqlx::query_scalar("SELECT analysis_status FROM contracts WHERE id=$1")
        .bind(cid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(s, "completed");
}
