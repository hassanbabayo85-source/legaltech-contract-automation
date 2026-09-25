//! Integration tests for the PART 03 authentication layer.
//!
//! Run against a real PostgreSQL via `#[sqlx::test]`. If DATABASE_URL is
//! unset or unreachable, every test here fails loudly.

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

use lexhack_backend::api;
use lexhack_backend::auth::session;
use lexhack_backend::config::Config;
use lexhack_backend::state::AppState;

const TEST_IP: &str = "127.0.0.1:5000";
const VALID_PASSWORD: &str = "correct horse battery staple";

fn test_config() -> Config {
    test_config_with_limits(1000, 1000)
}

fn test_config_with_limits(login_per_minute: u32, register_per_hour: u32) -> Config {
    let login_limit = login_per_minute.to_string();
    let register_limit = register_per_hour.to_string();
    let pairs: HashMap<&str, &str> = [
        ("DATABASE_URL", "postgres://test/test@localhost/test"),
        ("SESSION_LIFETIME_SECONDS", "3600"),
        ("ARGON2_M_COST", "19456"),
        ("ARGON2_T_COST", "2"),
        ("ARGON2_P_COST", "1"),
        ("RATE_LIMIT_LOGIN_PER_MINUTE", login_limit.as_str()),
        ("RATE_LIMIT_REGISTER_PER_HOUR", register_limit.as_str()),
    ]
    .into_iter()
    .collect();

    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string()))
        .expect("test config should be valid")
}

fn app(pool: PgPool) -> Router {
    api::build_router(AppState::new(test_config(), pool))
}

fn app_with_limits(pool: PgPool, login: u32, register: u32) -> Router {
    api::build_router(AppState::new(
        test_config_with_limits(login, register),
        pool,
    ))
}

fn request(method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let body = match body {
        Some(v) => Body::from(v.to_string()),
        None => Body::empty(),
    };
    let mut req = builder.body(body).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(TEST_IP.parse::<SocketAddr>().unwrap()));
    req
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn register_user(app: &Router, email: &str, password: &str, full_name: &str) -> Value {
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": email,
                "password": password,
                "full_name": full_name,
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "register should succeed"
    );
    json_body(response).await
}

// ---------- registration ----------

#[sqlx::test(migrations = "./migrations")]
async fn register_creates_user_and_returns_token(pool: PgPool) {
    let app = app(pool);
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice Example").await;

    assert_eq!(body["user"]["email"], "alice@example.com");
    assert_eq!(body["user"]["full_name"], "Alice Example");
    assert!(body["user"]["id"].is_string());
    assert!(body["token"].is_string());
    assert!(body["expires_at"].is_string());

    let as_str = body.to_string();
    assert!(!as_str.contains(VALID_PASSWORD));
    assert!(!as_str.contains("password_hash"));
}

#[sqlx::test(migrations = "./migrations")]
async fn register_stores_argon2id_hash_not_plaintext(pool: PgPool) {
    let app = app(pool.clone());
    register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice Example").await;

    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE email = $1")
        .bind("alice@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(
        hash.starts_with("$argon2id$"),
        "expected Argon2id PHC string"
    );
    assert_ne!(hash, VALID_PASSWORD);
    assert!(!hash.contains(VALID_PASSWORD));
}

#[sqlx::test(migrations = "./migrations")]
async fn register_rejects_duplicate_email(pool: PgPool) {
    let app = app(pool);
    register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice Example").await;

    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": "alice@example.com",
                "password": VALID_PASSWORD,
                "full_name": "Alice Again",
            })),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn register_normalizes_email_case_and_whitespace(pool: PgPool) {
    let app = app(pool.clone());
    register_user(
        &app,
        "  ALICE@Example.COM ",
        VALID_PASSWORD,
        "Alice Example",
    )
    .await;

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'alice@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn register_rejects_invalid_email(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": "not-an-email",
                "password": VALID_PASSWORD,
                "full_name": "Alice",
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn register_rejects_short_password(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": "alice@example.com",
                "password": "short",
                "full_name": "Alice",
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn register_rejects_empty_full_name(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": "alice@example.com",
                "password": VALID_PASSWORD,
                "full_name": "   ",
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------- login ----------

#[sqlx::test(migrations = "./migrations")]
async fn login_succeeds_with_correct_credentials(pool: PgPool) {
    let app = app(pool);
    register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;

    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/login",
            Some(json!({
                "email": "alice@example.com",
                "password": VALID_PASSWORD,
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["token"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn login_rejects_wrong_password(pool: PgPool) {
    let app = app(pool);
    register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;

    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/login",
            Some(json!({
                "email": "alice@example.com",
                "password": "wrong password entirely",
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_rejects_unknown_email_with_generic_error(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/login",
            Some(json!({
                "email": "nobody@example.com",
                "password": VALID_PASSWORD,
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_body(response).await;
    assert_eq!(body["error"]["message"], "invalid credentials");
}

// ---------- sessions ----------

#[sqlx::test(migrations = "./migrations")]
async fn session_token_is_hashed_at_rest(pool: PgPool) {
    let app = app(pool.clone());
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;
    let raw_token = body["token"].as_str().unwrap();

    let stored: String = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_ne!(stored, raw_token, "raw token must not be stored");
    assert_eq!(stored, session::hash_token(raw_token));
    assert_eq!(stored.len(), 64);
}

#[sqlx::test(migrations = "./migrations")]
async fn me_requires_authentication(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request("GET", "/api/auth/me", None, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn me_returns_current_user(pool: PgPool) {
    let app = app(pool);
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice Example").await;
    let token = body["token"].as_str().unwrap();

    let response = app
        .oneshot(request("GET", "/api/auth/me", None, Some(token)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let me = json_body(response).await;
    assert_eq!(me["email"], "alice@example.com");
    assert_eq!(me["full_name"], "Alice Example");
    assert!(me["password_hash"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_authorization_header_rejected(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request(
            "GET",
            "/api/auth/me",
            None,
            Some("not-a-real-token"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_session_is_rejected(pool: PgPool) {
    let app = app(pool.clone());
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;
    let user_id: Uuid = Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();

    // Insert a session that was created in the past and whose expiry
    // has already passed. The schema's `sessions_expires_after_created`
    // CHECK constraint requires `expires_at > created_at`, so both must
    // be set explicitly. A naive insert with `expires_at` in the past
    // but `created_at` defaulting to NOW() is correctly rejected by the
    // database — an expiry before creation is nonsensical.
    let raw = session::generate_token();
    let hash = session::hash_token(&raw);
    let created_at = Utc::now() - Duration::hours(2);
    let expires_at = Utc::now() - Duration::hours(1);
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at, created_at, last_used_at) \
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(user_id)
    .bind(&hash)
    .bind(expires_at)
    .bind(created_at)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(request("GET", "/api/auth/me", None, Some(&raw)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn revoked_session_is_rejected(pool: PgPool) {
    let app = app(pool.clone());
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;
    let raw = body["token"].as_str().unwrap().to_string();

    sqlx::query("UPDATE sessions SET revoked_at = NOW()")
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(request("GET", "/api/auth/me", None, Some(&raw)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn logout_revokes_session(pool: PgPool) {
    let app = app(pool.clone());
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;
    let token = body["token"].as_str().unwrap().to_string();

    let pre = app
        .clone()
        .oneshot(request("GET", "/api/auth/me", None, Some(&token)))
        .await
        .unwrap();
    assert_eq!(pre.status(), StatusCode::OK);

    let logout = app
        .clone()
        .oneshot(request("POST", "/api/auth/logout", None, Some(&token)))
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let post = app
        .oneshot(request("GET", "/api/auth/me", None, Some(&token)))
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::UNAUTHORIZED);
}

// ---------- rate limiting ----------

#[sqlx::test(migrations = "./migrations")]
async fn login_rate_limit_triggers_after_threshold(pool: PgPool) {
    let app = app_with_limits(pool, 2, 1000);

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/auth/login",
                Some(json!({
                    "email": "nobody@example.com",
                    "password": VALID_PASSWORD,
                })),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let response = app
        .oneshot(request(
            "POST",
            "/api/auth/login",
            Some(json!({
                "email": "nobody@example.com",
                "password": VALID_PASSWORD,
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().get(header::RETRY_AFTER).is_some());
}

// ---------- session cleanup ----------

#[sqlx::test(migrations = "./migrations")]
async fn delete_stale_sessions_removes_expired_and_old_revoked(pool: PgPool) {
    use lexhack_backend::db;

    let app = app(pool.clone());
    let body = register_user(&app, "alice@example.com", VALID_PASSWORD, "Alice").await;
    let user_id: Uuid = Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();

    // An expired session: created 2 hours ago, expired 1 hour ago.
    // Both `created_at` and `expires_at` are set explicitly to satisfy
    // the `sessions_expires_after_created` CHECK constraint.
    let expired_hash = session::hash_token(&session::generate_token());
    let created_at = Utc::now() - Duration::hours(2);
    let expires_at = Utc::now() - Duration::hours(1);
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at, created_at, last_used_at) \
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(user_id)
    .bind(&expired_hash)
    .bind(expires_at)
    .bind(created_at)
    .execute(&pool)
    .await
    .unwrap();

    // A long-revoked session.
    let revoked_hash = session::hash_token(&session::generate_token());
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at, revoked_at, created_at) \
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(user_id)
    .bind(&revoked_hash)
    .bind(Utc::now() + Duration::days(1))
    .bind(Utc::now() - Duration::days(30))
    .execute(&pool)
    .await
    .unwrap();

    let deleted = db::sessions::delete_stale_sessions(&pool, Utc::now() - Duration::days(7))
        .await
        .unwrap();
    assert!(
        deleted >= 2,
        "expected at least 2 rows removed, got {deleted}"
    );

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 1);
}
