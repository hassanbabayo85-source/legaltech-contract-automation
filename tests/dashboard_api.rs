//! Integration tests for `/api/dashboard/summary`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use lexhack_backend::api;
use lexhack_backend::config::Config;
use lexhack_backend::state::AppState;
use lexhack_backend::worker::poller::ChannelRegistry;

const TEST_IP: &str = "127.0.0.1:5000";
const VALID_PASSWORD: &str = "correct horse battery staple";

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
    ]
    .into_iter()
    .collect();
    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string())).unwrap()
}

#[derive(Debug)]
struct NoOpProvider;

#[async_trait::async_trait]
impl lexhack_backend::ai::AiProvider for NoOpProvider {
    fn name(&self) -> &'static str {
        "noop"
    }
    fn model(&self) -> &str {
        "noop"
    }
    async fn analyze_contract(
        &self,
        _: &str,
    ) -> Result<lexhack_backend::ai::schema::AiAnalysis, lexhack_backend::ai::AiError> {
        Err(lexhack_backend::ai::AiError::NotConfigured)
    }
}

fn app(pool: PgPool) -> Router {
    let channels = Arc::new(ChannelRegistry::test_stub());
    let provider: Arc<dyn lexhack_backend::ai::AiProvider> = Arc::new(NoOpProvider);
    api::build_router(AppState::with_provider(
        test_config(),
        pool,
        provider,
        channels,
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

#[sqlx::test(migrations = "./migrations")]
async fn summary_requires_authentication(pool: PgPool) {
    let app = app(pool);
    let r = app
        .oneshot(request("GET", "/api/dashboard/summary", None, None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn summary_returns_zeroes_for_new_user(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;

    let r = app
        .oneshot(request("GET", "/api/dashboard/summary", None, Some(&token)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;

    assert_eq!(b["contracts"]["total"], 0);
    assert_eq!(b["contracts"]["analyzed"], 0);
    assert_eq!(b["contracts"]["pending_analysis"], 0);
    assert_eq!(b["contracts"]["high_or_critical"], 0);
    assert_eq!(b["obligations"]["overdue"], 0);
    assert_eq!(b["reminders"]["pending"], 0);
    assert_eq!(b["reminders"]["sent"], 0);
    assert_eq!(b["reminders"]["failed"], 0);
    assert_eq!(b["reminders"]["upcoming_30d"], 0);
    assert_eq!(b["channels"]["enabled"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn summary_counts_own_contracts(pool: PgPool) {
    let app = app(pool.clone());
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;

    // Alice creates 2 contracts, Bob creates 1.
    for title in ["A1", "A2"] {
        let r = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/contracts",
                Some(json!({"title": title, "raw_text": "body"})),
                Some(&alice),
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
    }
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "B1", "raw_text": "body"})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);

    // Alice sees 2 total (not 3).
    let r = app
        .oneshot(request("GET", "/api/dashboard/summary", None, Some(&alice)))
        .await
        .unwrap();
    let b = json_body(r).await;
    assert_eq!(b["contracts"]["total"], 2);
    assert_eq!(b["contracts"]["pending_analysis"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn summary_counts_enabled_channels(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;

    // Create a channel.
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "T",
                "bot_token": "123456:ABC-DEF",
                "chat_id": "-100"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);

    let r = app
        .oneshot(request("GET", "/api/dashboard/summary", None, Some(&token)))
        .await
        .unwrap();
    let b = json_body(r).await;
    assert_eq!(b["channels"]["enabled"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn summary_is_scoped_per_user(pool: PgPool) {
    let app = app(pool);
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;

    // Alice has a channel; Bob has none.
    let _ = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "webhook",
                "name": "W",
                "url": "https://example.com/hook"
            })),
            Some(&alice),
        ))
        .await
        .unwrap();

    let r = app
        .oneshot(request("GET", "/api/dashboard/summary", None, Some(&bob)))
        .await
        .unwrap();
    let b = json_body(r).await;
    assert_eq!(
        b["channels"]["enabled"], 0,
        "Bob must not see Alice's channel"
    );
}

// Suppress unused import lint for Uuid in some builds.
#[allow(dead_code)]
fn _uses_uuid() -> Uuid {
    Uuid::nil()
}
