//! HTTP handlers for `/api/auth/*`.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{password, session, validation, AuthenticatedUser};
use crate::db;
use crate::errors::{AppError, AppResult};
use crate::models::User;
use crate::state::AppState;

// ---------- Shared response types ----------

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub created_at: DateTime<Utc>,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            email: u.email.clone(),
            full_name: u.full_name.clone(),
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user: UserResponse,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

// ---------- Rate limit helper ----------

fn check_rate_limit(
    limiter: &crate::middleware::rate_limit::RateLimiter,
    addr: &SocketAddr,
) -> Result<(), AppError> {
    limiter
        .check(addr.ip())
        .map_err(|retry_after| AppError::RateLimited {
            retry_after_seconds: retry_after.as_secs().max(1),
        })
}

// ---------- POST /api/auth/register ----------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub full_name: String,
}

pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<RegisterRequest>,
) -> AppResult<impl IntoResponse> {
    check_rate_limit(state.register_rate_limiter(), &addr)?;

    let email = validation::normalize_email(&body.email);
    validation::validate_email(&email)?;
    validation::validate_full_name(&body.full_name)?;
    validation::validate_password(&body.password)?;

    let password_hash = password::hash_password(state.config(), &body.password).map_err(|err| {
        tracing::error!(error = %err, "argon2 hashing failed");
        AppError::Internal(anyhow::anyhow!("password hashing failed"))
    })?;

    let user = match db::users::insert_user(
        state.db_pool(),
        &email,
        &password_hash,
        body.full_name.trim(),
    )
    .await
    {
        Ok(user) => user,
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            // Deliberate policy: registration DOES disclose duplicate
            // email (see docs/AUTH.md). Rate limiting mitigates mass
            // enumeration; the alternative (silent success) would
            // require email infrastructure we do not yet have.
            tracing::info!("registration rejected: duplicate email");
            return Err(AppError::Conflict(
                "an account with this email already exists".to_string(),
            ));
        }
        Err(err) => return Err(AppError::Database(err)),
    };

    let issued = session::issue_token(state.config().session_lifetime_seconds);
    db::sessions::insert_session(
        state.db_pool(),
        user.id,
        &issued.token_hash,
        issued.expires_at,
    )
    .await
    .map_err(AppError::Database)?;

    tracing::info!(user_id = %user.id, "user registered");

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user: UserResponse::from(&user),
            token: issued.raw_token,
            expires_at: issued.expires_at,
        }),
    ))
}

// ---------- POST /api/auth/login ----------

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// A fixed, valid Argon2id PHC string used to burn the same amount of
/// CPU as a real failed verification when the email does not exist. Its
/// plaintext is not known to anyone (the salt and hash were generated
/// once and discarded), so a successful verify against it is impossible.
const DUMMY_ARGON2_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$\
     AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    check_rate_limit(state.login_rate_limiter(), &addr)?;

    let email = validation::normalize_email(&body.email);
    validation::validate_email(&email)?;

    // Deliberately generic on failure — no distinction between "unknown
    // email" and "wrong password" is exposed.
    let generic = || AppError::Authentication("invalid credentials".to_string());

    let user = match db::users::find_user_by_email(state.db_pool(), &email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            // Burn the same CPU as a failed verification to reduce the
            // timing signal that would otherwise reveal account existence.
            let _ = password::verify_password(state.config(), &body.password, DUMMY_ARGON2_HASH);
            tracing::info!("login failed: unknown email");
            return Err(generic());
        }
        Err(err) => return Err(AppError::Database(err)),
    };

    if !password::verify_password(state.config(), &body.password, &user.password_hash) {
        tracing::info!(user_id = %user.id, "login failed: wrong password");
        return Err(generic());
    }

    let issued = session::issue_token(state.config().session_lifetime_seconds);
    db::sessions::insert_session(
        state.db_pool(),
        user.id,
        &issued.token_hash,
        issued.expires_at,
    )
    .await
    .map_err(AppError::Database)?;

    tracing::info!(user_id = %user.id, "login succeeded");

    Ok((
        StatusCode::OK,
        Json(AuthResponse {
            user: UserResponse::from(&user),
            token: issued.raw_token,
            expires_at: issued.expires_at,
        }),
    ))
}

// ---------- POST /api/auth/logout ----------

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> AppResult<impl IntoResponse> {
    db::sessions::revoke_session(state.db_pool(), auth.session_id)
        .await
        .map_err(AppError::Database)?;

    tracing::info!(
        user_id = %auth.user_id,
        session_id = %auth.session_id,
        "logout"
    );

    Ok(StatusCode::NO_CONTENT)
}

// ---------- GET /api/auth/me ----------

pub async fn me(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> AppResult<impl IntoResponse> {
    let user = db::users::find_user_by_id(state.db_pool(), auth.user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::Authentication("account no longer exists".to_string()))?;

    Ok((StatusCode::OK, Json(UserResponse::from(&user))))
}
