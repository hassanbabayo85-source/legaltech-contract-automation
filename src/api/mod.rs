//! HTTP API layer.

mod channels;
mod contracts;
mod dashboard;
mod health;
mod reminders;

use axum::Router;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(health::router())
        .nest("/api", api_router())
        .with_state(state)
}

fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/auth", crate::auth::router::router())
        .nest("/contracts", contracts::router())
        .nest("/reminders", reminders::standalone_router())
        .nest("/notification-channels", channels::router())
        .nest("/dashboard", dashboard::router())
}
