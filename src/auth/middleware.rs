//! The `AuthenticatedUser` extractor.
//!
//! Any handler that receives `AuthenticatedUser` is guaranteed to be
//! running behind a valid, non-revoked, non-expired session for a real
//! user. This is the *only* channel through which a handler should learn
//! the caller's identity — handlers must never read an identity out of
//! the request body or query string.
//!
//! # Why `#[async_trait]`
//!
//! Axum 0.7 defines `FromRequestParts::from_request_parts` as an
//! `async fn` in a trait (AFIT). Writing the impl with a plain
//! `async fn` desugars the function's implicit lifetime parameters in a
//! way that does not match the trait declaration, producing `E0195`
//! (lifetime parameters do not match). The canonical fix — used by
//! axum's own extractors (`State`, `Json`, `Query`, …) — is to apply
//! `#[async_trait]` to the impl. The macro desugars both sides to the
//! same shape.
//!
//! # Why the extractor is generic over `S`
//!
//! `FromRequestParts` is generic over the router's state type `S`. We
//! accept any `S` from which an `AppState` can be obtained
//! (`AppState: FromRef<S>`), which is how axum threads shared state
//! through extractors. For our router `S = AppState`, and
//! `AppState: FromRef<AppState>` holds through axum's blanket
//! `impl<T: Clone> FromRef<T> for T`.

use async_trait::async_trait;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::auth::session::hash_token;
use crate::db;
use crate::errors::AppError;
use crate::state::AppState;

/// Identity of the currently authenticated user.
///
/// Both `user_id` and `session_id` are needed: `user_id` for ownership
/// checks in resource handlers, `session_id` so logout and future
/// bulk-revocation can identify the exact session.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);

        // 1. Extract the Authorization header and require the Bearer
        //    scheme. Every failure below deliberately maps to the same
        //    generic message so the client cannot distinguish "missing
        //    header" from "unknown token" from "expired token".
        let generic = || AppError::Authentication("invalid or missing credentials".to_string());

        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(generic)?;

        let raw_token = header
            .strip_prefix("Bearer ")
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(generic)?;

        // 2. Hash the token and look up an active session. The query
        //    filters out revoked and expired sessions; either yields
        //    `None`, which maps to the same rejection.
        let token_hash = hash_token(raw_token);
        let session = db::sessions::find_active_session_by_token_hash(state.db_pool(), &token_hash)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(generic)?;

        // 3. Best-effort touch of last_used_at. A failure here does not
        //    invalidate the request.
        if let Err(err) = db::sessions::touch_session(state.db_pool(), session.id).await {
            tracing::warn!(error = ?err, "failed to update session last_used_at");
        }

        Ok(AuthenticatedUser {
            user_id: session.user_id,
            session_id: session.id,
        })
    }
}
