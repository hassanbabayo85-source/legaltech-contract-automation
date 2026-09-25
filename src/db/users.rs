//! Minimal typed access to the `users` table.

use crate::models::User;
use uuid::Uuid;

/// Inserts a new user and returns the stored row.
///
/// `password_hash` must already be a strong hash (Argon2id, applied by
/// `crate::auth::password`). This function never inspects or logs its
/// value.
pub async fn insert_user<'e, E>(
    executor: E,
    email: &str,
    password_hash: &str,
    full_name: &str,
) -> Result<User, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, User>(
        "INSERT INTO users (email, password_hash, full_name) \
         VALUES ($1, $2, $3) \
         RETURNING id, email, password_hash, full_name, created_at, updated_at",
    )
    .bind(email)
    .bind(password_hash)
    .bind(full_name)
    .fetch_one(executor)
    .await
}

/// Looks up a user by email. Email comparison is exact — callers must
/// normalize first (see `crate::auth::validation::normalize_email`).
pub async fn find_user_by_email<'e, E>(
    executor: E,
    email: &str,
) -> Result<Option<User>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, created_at, updated_at \
         FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(executor)
    .await
}

/// Looks up a user by id.
pub async fn find_user_by_id<'e, E>(executor: E, id: Uuid) -> Result<Option<User>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, created_at, updated_at \
         FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
}
