//! Database access for `notification_channels`.
//!
//! All reads and writes enforce ownership through `user_id`. Secrets
//! are encrypted/decrypted by [`crate::notifications::secrets`]; this
//! module never sees plaintext beyond the encryption boundary.

use uuid::Uuid;

use crate::models::NotificationChannel;
use crate::notifications::config::ChannelConfig;
use crate::notifications::secrets::{SecretError, SecretKey};
use crate::reminders::types::ChannelType;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("secret error: {0}")]
    Secret(#[from] SecretError),

    #[error("stored config is not valid JSON: {0}")]
    ConfigJson(String),
}

/// Row type alias for the tuple with encrypted_config.
type ChannelWithEncryptedConfig = (
    Uuid,
    Uuid,
    String,
    String,
    bool,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<String>,
    Vec<u8>,
);

pub async fn insert_channel<'e, E>(
    executor: E,
    user_id: Uuid,
    channel_type: ChannelType,
    name: &str,
    config: &ChannelConfig,
    secret_key: &SecretKey,
) -> Result<NotificationChannel, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let json = serde_json::to_vec(config).map_err(|e| StoreError::ConfigJson(e.to_string()))?;
    let encrypted = secret_key.encrypt(&json)?;

    let row = sqlx::query_as::<_, NotificationChannel>(
        "INSERT INTO notification_channels \
             (user_id, channel_type, name, encrypted_config) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, user_id, channel_type, name, enabled, \
                   created_at, updated_at, last_tested_at, last_test_error",
    )
    .bind(user_id)
    .bind(channel_type.as_str())
    .bind(name)
    .bind(&encrypted)
    .fetch_one(executor)
    .await?;

    Ok(row)
}

pub async fn find_channel_for_user<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<NotificationChannel>, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let row = sqlx::query_as::<_, NotificationChannel>(
        "SELECT id, user_id, channel_type, name, enabled, \
                created_at, updated_at, last_tested_at, last_test_error \
         FROM notification_channels \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

pub async fn find_channel_with_config_for_user<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
    secret_key: &SecretKey,
) -> Result<Option<(NotificationChannel, ChannelConfig)>, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let row: Option<ChannelWithEncryptedConfig> = sqlx::query_as(
        "SELECT id, user_id, channel_type, name, enabled, \
                created_at, updated_at, last_tested_at, last_test_error, \
                encrypted_config \
         FROM notification_channels \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(executor)
    .await?;

    let Some((
        id,
        user_id,
        channel_type,
        name,
        enabled,
        created_at,
        updated_at,
        last_tested_at,
        last_test_error,
        encrypted,
    )) = row
    else {
        return Ok(None);
    };

    let decrypted = secret_key.decrypt(&encrypted)?;
    let config: ChannelConfig =
        serde_json::from_slice(&decrypted).map_err(|e| StoreError::ConfigJson(e.to_string()))?;

    Ok(Some((
        NotificationChannel {
            id,
            user_id,
            channel_type,
            name,
            enabled,
            created_at,
            updated_at,
            last_tested_at,
            last_test_error,
        },
        config,
    )))
}

pub async fn list_channels_for_user<'e, E>(
    executor: E,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<NotificationChannel>, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, NotificationChannel>(
        "SELECT id, user_id, channel_type, name, enabled, \
                created_at, updated_at, last_tested_at, last_test_error \
         FROM notification_channels \
         WHERE user_id = $1 \
         ORDER BY created_at DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

pub async fn count_channels_for_user<'e, E>(executor: E, user_id: Uuid) -> Result<i64, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let n = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM notification_channels WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(executor)
    .await?;
    Ok(n)
}

pub async fn update_channel<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
    name: Option<&str>,
    enabled: Option<bool>,
    config: Option<&ChannelConfig>,
    secret_key: &SecretKey,
) -> Result<Option<NotificationChannel>, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let encrypted = match config {
        Some(cfg) => {
            let json =
                serde_json::to_vec(cfg).map_err(|e| StoreError::ConfigJson(e.to_string()))?;
            Some(secret_key.encrypt(&json)?)
        }
        None => None,
    };

    let row = sqlx::query_as::<_, NotificationChannel>(
        "UPDATE notification_channels SET \
             name = COALESCE($3, name), \
             enabled = COALESCE($4, enabled), \
             encrypted_config = COALESCE($5, encrypted_config), \
             updated_at = NOW() \
         WHERE id = $1 AND user_id = $2 \
         RETURNING id, user_id, channel_type, name, enabled, \
                   created_at, updated_at, last_tested_at, last_test_error",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(enabled)
    .bind(encrypted.as_deref())
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

pub async fn delete_channel<'e, E>(executor: E, id: Uuid, user_id: Uuid) -> Result<bool, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let n = sqlx::query("DELETE FROM notification_channels WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(executor)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// Finds the enabled channel of a given type for a user, returning its
/// encrypted config so the caller can decrypt with the shared key.
pub async fn find_enabled_channel_for_user_by_type<'e, E>(
    executor: E,
    user_id: Uuid,
    channel_type: ChannelType,
) -> Result<Option<(NotificationChannel, Vec<u8>)>, StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let row: Option<ChannelWithEncryptedConfig> = sqlx::query_as(
        "SELECT id, user_id, channel_type, name, enabled, \
                created_at, updated_at, last_tested_at, last_test_error, \
                encrypted_config \
         FROM notification_channels \
         WHERE user_id = $1 AND channel_type = $2 AND enabled = TRUE \
         LIMIT 1",
    )
    .bind(user_id)
    .bind(channel_type.as_str())
    .fetch_optional(executor)
    .await?;

    Ok(row.map(
        |(
            id,
            user_id,
            channel_type,
            name,
            enabled,
            created_at,
            updated_at,
            last_tested_at,
            last_test_error,
            encrypted_config,
        )| {
            (
                NotificationChannel {
                    id,
                    user_id,
                    channel_type,
                    name,
                    enabled,
                    created_at,
                    updated_at,
                    last_tested_at,
                    last_test_error,
                },
                encrypted_config,
            )
        },
    ))
}

pub async fn record_test_result<'e, E>(
    executor: E,
    id: Uuid,
    user_id: Uuid,
    error: Option<&str>,
) -> Result<(), StoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "UPDATE notification_channels SET \
             last_tested_at = NOW(), \
             last_test_error = $3, \
             updated_at = NOW() \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .bind(error)
    .execute(executor)
    .await?;
    Ok(())
}
