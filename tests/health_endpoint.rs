//! Integration tests for the HTTP layer.
//!
//! Uses `Config::from_lookup` (not `Config::load`) so the tests do not
//! depend on the process environment — a production deployment that
//! forgets `NOTIFICATION_SECRET_KEY` should fail at startup, but a
//! unit test of the health endpoint should not need one.

use std::collections::HashMap;

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lexhack_backend::api;
use lexhack_backend::config::Config;
use lexhack_backend::state::AppState;
use lexhack_backend::{db, worker::poller::ChannelRegistry};

use std::sync::Arc;

/// Test configuration built entirely in memory.
#[allow(dead_code)] // used only in some build configurations
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
    Config::from_lookup(|k| pairs.get(k).map(|v| v.to_string())).expect("test config valid")
}

#[tokio::test]
async fn health_endpoint_returns_ok_through_full_router() {
    let Some(state) = app_state().await else {
        return;
    };
    let app = api::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let Some(state) = app_state().await else {
        return;
    };
    let app = api::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Builds `AppState` for an integration test.
///
/// Returns `None` **only** when the caller has explicitly opted out via
/// `LEXHACK_ALLOW_MISSING_DB=1`; in every other failure mode this
/// panics.
async fn app_state() -> Option<AppState> {
    if std::env::var("LEXHACK_ALLOW_MISSING_DB").as_deref() == Ok("1") {
        eprintln!("warning: LEXHACK_ALLOW_MISSING_DB=1 — skipping DB integration test");
        return None;
    }

    // The DB pool needs a live DATABASE_URL from the environment.
    let config = Config::from_lookup(|key| {
        std::env::var(key).ok().or_else(|| {
            // Fall back to test defaults for anything the env doesn't provide.
            let map: HashMap<&str, &str> = [
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
            map.get(key).map(|v| v.to_string())
        })
    })
    .unwrap_or_else(|err| {
        panic!(
            "integration test could not load configuration: {err}. \
             Set DATABASE_URL (and any other required variables), or set \
             LEXHACK_ALLOW_MISSING_DB=1 to explicitly skip."
        )
    });

    let pool = db::init_pool(&config).await.unwrap_or_else(|err| {
        panic!(
            "integration test could not reach PostgreSQL: {err}. \
             Set DATABASE_URL, or set LEXHACK_ALLOW_MISSING_DB=1 to explicitly skip."
        )
    });

    let channels = Arc::new(ChannelRegistry::test_stub());
    let provider: Arc<dyn lexhack_backend::ai::AiProvider> = Arc::new(NoOpProvider);
    Some(AppState::with_provider(config, pool, provider, channels))
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
