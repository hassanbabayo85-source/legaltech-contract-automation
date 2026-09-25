//! Database connection infrastructure.
//!
//! Owns the connection pool (established in PART 01) and the minimal set
//! of typed helpers required to verify the PART 02 schema and support the
//! PART 03 authentication layer.
//!
//! # Security
//!
//! These functions never log their arguments. Errors returned to the
//! caller are `sqlx::Error` values, which — because `AppError::Database`
//! maps them to a generic message at the HTTP layer and
//! `AppError::log_message` reduces them to a category string — never
//! carry row data into logs.

pub mod audit;
pub mod contracts;
pub mod dashboard;
pub mod obligations;
pub mod reminders;
pub mod risks;
pub mod sessions;
pub mod users;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::time::Duration;

use crate::config::Config;

/// Creates a PostgreSQL connection pool from application configuration.
///
/// Fails clearly (rather than hanging) if the database cannot be reached,
/// so that the application does not silently start in a broken state.
pub async fn init_pool(config: &Config) -> Result<PgPool, sqlx::Error> {
    let connect_options: PgConnectOptions = config.database_url().parse().map_err(|err| {
        tracing::error!(error = %err, "failed to parse DATABASE_URL");
        err
    })?;

    tracing::info!("initializing database connection pool");

    let pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(connect_options)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "failed to connect to the database");
            err
        })?;

    tracing::info!("database connection pool established");

    Ok(pool)
}
