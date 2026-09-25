//! `/api/auth/*` router.

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

use crate::auth::handlers;
use crate::state::AppState;

/// Builds the auth router. Applies an 8 KiB body limit — well below the
/// 2 MiB Axum default — because none of these endpoints ever needs a
/// large payload, and a tight limit reduces the blast radius of an
/// accidental DoS.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
        .route("/logout", post(handlers::logout))
        .route("/me", get(handlers::me))
        .layer(DefaultBodyLimit::max(8 * 1024))
}
