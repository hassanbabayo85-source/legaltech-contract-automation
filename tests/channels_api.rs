//! Integration tests for the PART 08 notification-channel API.
//!
//! Every test drives the full router and PostgreSQL. Secrets never
//! appear in any API response; ownership is enforced on every
//! operation.

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

// A trivial AI provider for these tests — never called.
#[derive(Debug)]
struct StubProvider;

#[async_trait::async_trait]
impl lexhack_backend::ai::AiProvider for StubProvider {
    fn name(&self) -> &'static str {
        "stub"
    }
    fn model(&self) -> &str {
        "stub"
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
    let provider: Arc<dyn lexhack_backend::ai::AiProvider> = Arc::new(StubProvider);
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

async fn create_telegram(app: &Router, token: &str) -> Value {
    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "My Telegram",
                "bot_token": "123456:ABC-DEF-secret-token",
                "chat_id": "-1001234567890"
            })),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    json_body(r).await
}

// -------------------------------------------------------------------------
// CREATE
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_requires_authentication(pool: PgPool) {
    let app = app(pool);
    let r = app
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "T",
                "bot_token": "1:a",
                "chat_id": "-1"
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_telegram_succeeds_and_hides_secret(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let body = create_telegram(&app, &token).await;

    assert_eq!(body["channel_type"], "telegram");
    assert_eq!(body["name"], "My Telegram");
    assert_eq!(body["enabled"], true);
    assert!(body["id"].is_string());
    assert!(body["created_at"].is_string());

    // Secret must NOT be present:
    let serialized = body.to_string();
    assert!(!serialized.contains("bot_token"));
    assert!(!serialized.contains("ABC-DEF-secret-token"));
    assert!(!serialized.contains("chat_id"));
    assert!(!serialized.contains("-1001234567890"));
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_missing_name(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let r = app
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "   ",
                "bot_token": "1:a",
                "chat_id": "-1"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_invalid_telegram_token(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let r = app
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "T",
                "bot_token": "no-colon",
                "chat_id": "-1"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_invalid_channel_type(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let r = app
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "carrier_pigeon",
                "name": "T"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    // serde rejects the variant -> unprocessable entity.
    assert!(
        r.status() == StatusCode::UNPROCESSABLE_ENTITY || r.status() == StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_enabled_channel_of_same_type_rejected(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let _ = create_telegram(&app, &token).await;

    let r = app
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "Second",
                "bot_token": "1:a",
                "chat_id": "-1"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

// -------------------------------------------------------------------------
// LIST / GET
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_only_own_channels(pool: PgPool) {
    let app = app(pool);
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;

    let _ = create_telegram(&app, &alice).await;

    let r = app
        .clone()
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
    assert_eq!(b["total"], 0);
    assert!(b["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_404_for_foreign_channel(pool: PgPool) {
    let app = app(pool);
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let created = create_telegram(&app, &alice).await;
    let id = created["id"].as_str().unwrap();

    let r = app
        .oneshot(request(
            "GET",
            &format!("/api/notification-channels/{id}"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_rejects_malformed_uuid(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let r = app
        .oneshot(request(
            "GET",
            "/api/notification-channels/not-a-uuid",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

// -------------------------------------------------------------------------
// UPDATE
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn patch_rename_keeps_secret(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "a@example.com").await;
    let created = create_telegram(&app, &token).await;
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // Grab the encrypted blob before update.
    let before: Vec<u8> =
        sqlx::query_scalar("SELECT encrypted_config FROM notification_channels WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "PATCH",
            &format!("/api/notification-channels/{id}"),
            Some(json!({"name": "Renamed"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["name"], "Renamed");

    // Secret bytes are unchanged (a re-encryption would have produced
    // a different nonce, even with the same plaintext).
    let after: Vec<u8> =
        sqlx::query_scalar("SELECT encrypted_config FROM notification_channels WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after, "rename must not touch encrypted_config");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_new_config_replaces_encrypted_blob(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "a@example.com").await;
    let created = create_telegram(&app, &token).await;
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let before: Vec<u8> =
        sqlx::query_scalar("SELECT encrypted_config FROM notification_channels WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "PATCH",
            &format!("/api/notification-channels/{id}"),
            Some(json!({
                "channel_type": "telegram",
                "bot_token": "999:NEW-SECRET",
                "chat_id": "-1009999999999"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);

    let after: Vec<u8> =
        sqlx::query_scalar("SELECT encrypted_config FROM notification_channels WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(before, after, "new config must replace encrypted blob");

    // Response still does not leak the new token.
    let b = json_body(
        app.oneshot(request(
            "GET",
            &format!("/api/notification-channels/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(!b.to_string().contains("999:NEW-SECRET"));
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_foreign_channel_denied(pool: PgPool) {
    let app = app(pool);
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let created = create_telegram(&app, &alice).await;
    let id = created["id"].as_str().unwrap();

    let r = app
        .oneshot(request(
            "PATCH",
            &format!("/api/notification-channels/{id}"),
            Some(json!({"name": "Hijacked"})),
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

// -------------------------------------------------------------------------
// DELETE
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_own_channel(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "a@example.com").await;
    let created = create_telegram(&app, &token).await;
    let id = created["id"].as_str().unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/notification-channels/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_foreign_channel_denied(pool: PgPool) {
    let app = app(pool.clone());
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let created = create_telegram(&app, &alice).await;
    let id = created["id"].as_str().unwrap();

    let r = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/notification-channels/{id}"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_channels")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "another user's channel must survive");
}

// -------------------------------------------------------------------------
// Disabled channel
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn disabled_channel_cannot_be_tested(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "a@example.com").await;
    let created = create_telegram(&app, &token).await;
    let id = created["id"].as_str().unwrap();

    // Disable it.
    let r = app
        .clone()
        .oneshot(request(
            "PATCH",
            &format!("/api/notification-channels/{id}"),
            Some(json!({"enabled": false})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);

    let r = app
        .oneshot(request(
            "POST",
            &format!("/api/notification-channels/{id}/test"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn disabled_channel_allows_second_of_same_type(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;
    let created = create_telegram(&app, &token).await;
    let id = created["id"].as_str().unwrap();

    // Disable first.
    let _ = app
        .clone()
        .oneshot(request(
            "PATCH",
            &format!("/api/notification-channels/{id}"),
            Some(json!({"enabled": false})),
            Some(&token),
        ))
        .await
        .unwrap();

    // Now a second telegram can be created.
    let r = app
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "telegram",
                "name": "Second",
                "bot_token": "2:b",
                "chat_id": "-2"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
}

// -------------------------------------------------------------------------
// SSRF through channel test (webhook with loopback URL)
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn webhook_channel_test_blocks_loopback(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "webhook",
                "name": "Loopback",
                "url": "https://127.0.0.1:9999/hook"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let id = json_body(r).await["id"].as_str().unwrap().to_string();

    let r = app
        .oneshot(request(
            "POST",
            &format!("/api/notification-channels/{id}/test"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["ok"], false);
    assert_eq!(b["error"], "ssrf_blocked");
}

#[sqlx::test(migrations = "./migrations")]
async fn webhook_channel_test_blocks_metadata(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "a@example.com").await;

    let r = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/notification-channels",
            Some(json!({
                "channel_type": "webhook",
                "name": "Metadata",
                "url": "https://169.254.169.254/latest/meta-data/"
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let id = json_body(r).await["id"].as_str().unwrap().to_string();

    let r = app
        .oneshot(request(
            "POST",
            &format!("/api/notification-channels/{id}/test"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b = json_body(r).await;
    assert_eq!(b["ok"], false);
    assert_eq!(b["error"], "ssrf_blocked");
}

// -------------------------------------------------------------------------
// Cross-user test endpoint
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn test_endpoint_rejects_foreign_channel(pool: PgPool) {
    let app = app(pool);
    let alice = register(&app, "a@example.com").await;
    let bob = register(&app, "b@example.com").await;
    let created = create_telegram(&app, &alice).await;
    let id = created["id"].as_str().unwrap();

    let r = app
        .oneshot(request(
            "POST",
            &format!("/api/notification-channels/{id}/test"),
            None,
            Some(&bob),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}
