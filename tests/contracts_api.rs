//! Integration tests for the PART 04 contract API.
//!
//! Every test drives the full Axum router: HTTP request ->
//! authentication -> authorization -> validation -> service ->
//! database -> HTTP response. Requires a live PostgreSQL instance via
//! `#[sqlx::test]`. If `DATABASE_URL` is unset or the database is
//! unreachable, every test fails loudly — they never silently skip.

use std::collections::HashMap;
use std::net::SocketAddr;

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
    ]
    .into_iter()
    .collect();
    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string())).unwrap()
}

fn app(pool: PgPool) -> Router {
    api::build_router(AppState::new(test_config(), pool))
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

async fn register(app: &Router, email: &str) -> String {
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/auth/register",
            Some(json!({
                "email": email,
                "password": VALID_PASSWORD,
                "full_name": "Test User",
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_contract(app: &Router, token: &str, title: &str, raw_text: &str) -> Value {
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": title, "raw_text": raw_text})),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create should succeed"
    );
    json_body(response).await
}

// -------------------------------------------------------------------------
// CREATE
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn create_requires_authentication(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "T", "raw_text": "R"})),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_succeeds_for_authenticated_user(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let body = create_contract(&app, &token, "Employment Agreement", "The parties agree...").await;

    assert_eq!(body["title"], "Employment Agreement");
    assert_eq!(body["raw_text"], "The parties agree...");
    assert_eq!(body["content_version"], 1);
    assert_eq!(body["analysis_status"], "not_analyzed");
    assert!(body["id"].is_string());
    assert!(body["created_at"].is_string());
    assert!(body["start_date"].is_null());
    assert!(body["risk_level"].is_null());

    // Ownership / secret leakage:
    assert!(body.get("user_id").is_none());
    assert!(body.get("password_hash").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_empty_title(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "   ", "raw_text": "R"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_empty_raw_text(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "T", "raw_text": "   \n  "})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_oversized_title(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let long = "a".repeat(600);
    let response = app
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": long, "raw_text": "R"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_client_supplied_user_id(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({
                "title": "T",
                "raw_text": "R",
                "user_id": "00000000-0000-0000-0000-000000000000",
            })),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn created_contract_belongs_to_authenticated_user(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let body = create_contract(&app, &token, "T", "R").await;
    let contract_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();

    let alice_id: Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'alice@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let owner: Uuid = sqlx::query_scalar("SELECT user_id FROM contracts WHERE id = $1")
        .bind(contract_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, alice_id);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_writes_audit_record(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let body = create_contract(&app, &token, "T", "secret contract text").await;
    let contract_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();

    let action: String = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE entity_id = $1 AND action = 'contract_created'",
    )
    .bind(contract_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(action, "contract_created");

    let metadata: Value = sqlx::query_scalar(
        "SELECT metadata FROM audit_logs WHERE entity_id = $1 AND action = 'contract_created'",
    )
    .bind(contract_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let as_str = metadata.to_string();
    assert!(!as_str.contains("secret contract text"));
}

// -------------------------------------------------------------------------
// LIST
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_requires_authentication(pool: PgPool) {
    let app = app(pool);
    let response = app
        .oneshot(request("GET", "/api/contracts", None, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_returns_only_own_contracts(pool: PgPool) {
    let app = app(pool);
    let token_a = register(&app, "alice@example.com").await;
    let token_b = register(&app, "bob@example.com").await;

    create_contract(&app, &token_a, "Alice's", "A").await;
    create_contract(&app, &token_b, "Bob's", "B").await;
    create_contract(&app, &token_b, "Bob's 2", "B2").await;

    let response = app
        .oneshot(request("GET", "/api/contracts", None, Some(&token_a)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;

    assert_eq!(body["total"], 1);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["title"], "Alice's");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_does_not_return_raw_text(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    create_contract(&app, &token, "T", "sensitive legal text").await;

    let response = app
        .oneshot(request("GET", "/api/contracts", None, Some(&token)))
        .await
        .unwrap();
    let body = json_body(response).await;
    let item = &body["items"][0];
    assert!(item.get("raw_text").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn list_pagination_defaults_and_metadata(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    for i in 0..3 {
        create_contract(&app, &token, &format!("C{i}"), "R").await;
    }

    let response = app
        .oneshot(request("GET", "/api/contracts", None, Some(&token)))
        .await
        .unwrap();
    let body = json_body(response).await;
    assert_eq!(body["page"], 1);
    assert_eq!(body["limit"], 20);
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_honours_limit(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    for i in 0..5 {
        create_contract(&app, &token, &format!("C{i}"), "R").await;
    }

    let response = app
        .oneshot(request("GET", "/api/contracts?limit=2", None, Some(&token)))
        .await
        .unwrap();
    let body = json_body(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"], 5);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_rejects_limit_above_maximum(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "GET",
            "/api/contracts?limit=100000",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_rejects_invalid_sort(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "GET",
            "/api/contracts?sort=evil;%20DROP%20TABLE%20users",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_rejects_invalid_order(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "GET",
            "/api/contracts?order=sideways",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_accepts_whitelisted_sort(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    create_contract(&app, &token, "B", "R").await;
    create_contract(&app, &token, "A", "R").await;

    let response = app
        .oneshot(request(
            "GET",
            "/api/contracts?sort=title&order=asc",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["items"][0]["title"], "A");
    assert_eq!(body["items"][1]["title"], "B");
}

// -------------------------------------------------------------------------
// GET single
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_own_contract_with_raw_text(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "exact sensitive text").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["raw_text"], "exact sensitive text");
}

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_404_for_foreign_contract(pool: PgPool) {
    let app = app(pool);
    let token_a = register(&app, "alice@example.com").await;
    let token_b = register(&app, "bob@example.com").await;
    let created = create_contract(&app, &token_a, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token_b),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_returns_404_for_nonexistent_contract(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let id = Uuid::new_v4();
    let response = app
        .oneshot(request(
            "GET",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_rejects_malformed_uuid(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let response = app
        .oneshot(request(
            "GET",
            "/api/contracts/not-a-uuid",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// -------------------------------------------------------------------------
// UPDATE
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn update_title_only(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "Old", "body").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"title": "New"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["title"], "New");
    assert_eq!(body["raw_text"], "body");
    assert_eq!(body["content_version"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_raw_text_bumps_content_version_and_resets_analysis(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "old body").await;
    let id = created["id"].as_str().unwrap();
    let contract_id = Uuid::parse_str(id).unwrap();

    sqlx::query(
        "UPDATE contracts SET analysis_status = 'completed', risk_level = 'high' WHERE id = $1",
    )
    .bind(contract_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"raw_text": "new body"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["raw_text"], "new body");
    assert_eq!(body["content_version"], 2);
    assert_eq!(body["analysis_status"], "not_analyzed");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_with_same_raw_text_does_not_bump_version(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "same body").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"raw_text": "same body"})),
            Some(&token),
        ))
        .await
        .unwrap();
    let body = json_body(response).await;
    assert_eq!(body["content_version"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rejects_foreign_contract(pool: PgPool) {
    let app = app(pool);
    let token_a = register(&app, "alice@example.com").await;
    let token_b = register(&app, "bob@example.com").await;
    let created = create_contract(&app, &token_a, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"title": "stolen"})),
            Some(&token_b),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rejects_client_supplied_user_id(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"user_id": "00000000-0000-0000-0000-000000000000"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_rejects_client_controlled_ai_fields(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"risk_level": "critical"})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_requires_at_least_one_field(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_changes_updated_at(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = created["id"].as_str().unwrap();
    let original_updated_at = created["updated_at"].as_str().unwrap().to_string();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let response = app
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"title": "New"})),
            Some(&token),
        ))
        .await
        .unwrap();
    let body = json_body(response).await;
    let new_updated_at = body["updated_at"].as_str().unwrap().to_string();
    assert_ne!(original_updated_at, new_updated_at);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_writes_audit_record(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = created["id"].as_str().unwrap();
    let contract_id = Uuid::parse_str(id).unwrap();

    let _ = app
        .clone()
        .oneshot(request(
            "PATCH",
            &format!("/api/contracts/{id}"),
            Some(json!({"raw_text": "updated body"})),
            Some(&token),
        ))
        .await
        .unwrap();

    let metadata: Value = sqlx::query_scalar(
        "SELECT metadata FROM audit_logs WHERE entity_id = $1 AND action = 'contract_updated'",
    )
    .bind(contract_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let changed = metadata["changed_fields"].as_array().unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0], "raw_text");
}

// -------------------------------------------------------------------------
// DELETE
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn delete_removes_own_contract(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_rejects_foreign_contract(pool: PgPool) {
    let app = app(pool.clone());
    let token_a = register(&app, "alice@example.com").await;
    let token_b = register(&app, "bob@example.com").await;
    let created = create_contract(&app, &token_a, "T", "R").await;
    let id = created["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token_b),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_nonexistent_returns_404(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    let id = Uuid::new_v4();
    let response = app
        .oneshot(request(
            "DELETE",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_cascades_to_dependent_rows(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // Seed dependent rows to exercise the CASCADE.
    let obligation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO contract_obligations (id, contract_id, title, description) \
         VALUES ($1, $2, 'T', 'D')",
    )
    .bind(obligation_id)
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO contract_risks (contract_id, title, description, risk_level, risk_score, evidence) \
         VALUES ($1, 'R', 'D', 'low', 10, 'E')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO reminders (contract_id, obligation_id, reminder_date, reminder_type, channel_type) \
         VALUES ($1, $2, NOW() + interval '1 day', 'on_deadline', 'telegram')",
    )
    .bind(id)
    .bind(obligation_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/contracts/{id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let deps: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM contract_obligations WHERE contract_id = $1), \
            (SELECT COUNT(*) FROM contract_risks WHERE contract_id = $1), \
            (SELECT COUNT(*) FROM reminders WHERE contract_id = $1)",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(deps, (0, 0, 0));
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_writes_audit_record(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    let created = create_contract(&app, &token, "T", "R").await;
    let contract_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let _ = app
        .clone()
        .oneshot(request(
            "DELETE",
            &format!("/api/contracts/{contract_id}"),
            None,
            Some(&token),
        ))
        .await
        .unwrap();

    let action: String = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE entity_id = $1 AND action = 'contract_deleted'",
    )
    .bind(contract_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(action, "contract_deleted");
}

// -------------------------------------------------------------------------
// SECURITY
// -------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn ownership_cannot_be_bypassed_by_id_guess(pool: PgPool) {
    let app = app(pool);
    let token_a = register(&app, "alice@example.com").await;
    let token_b = register(&app, "bob@example.com").await;
    let created = create_contract(&app, &token_a, "Alice's", "sensitive").await;
    let id = created["id"].as_str().unwrap();

    for method in ["GET", "PATCH", "DELETE"] {
        let body = if method == "PATCH" {
            Some(json!({"title": "stolen"}))
        } else {
            None
        };
        let response = app
            .clone()
            .oneshot(request(
                method,
                &format!("/api/contracts/{id}"),
                body,
                Some(&token_b),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} as Bob against Alice's contract should 404"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn sql_injection_attempt_in_sort_fails_safely(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "alice@example.com").await;
    create_contract(&app, &token, "T", "R").await;

    let response = app
        .oneshot(request(
            "GET",
            "/api/contracts?sort=id;DROP%20TABLE%20users",
            None,
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // The `users` table must still exist.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_rejects_oversized_payload_at_http_layer(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "alice@example.com").await;
    // 5 MiB of text — over the 4 MiB router limit.
    let huge = "a".repeat(5 * 1024 * 1024);
    let response = app
        .oneshot(request(
            "POST",
            "/api/contracts",
            Some(json!({"title": "T", "raw_text": huge})),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
