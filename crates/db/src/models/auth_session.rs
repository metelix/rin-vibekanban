use chrono::{Duration, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct AuthSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub refresh_token_id: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub last_used_at: Option<String>,
}

const AUTH_SESSION_COLUMNS: &str =
    "id, user_id, refresh_token_id, created_at, expires_at, last_used_at";

pub async fn create(
    pool: &SqlitePool,
    id: Uuid,
    user_id: Uuid,
    refresh_token_id: &str,
    ttl: Duration,
) -> Result<AuthSession, sqlx::Error> {
    let now = Utc::now();
    let expires_at = now + ttl;

    let row: AuthSession = sqlx::query_as(&format!(
        "INSERT INTO auth_sessions ({AUTH_SESSION_COLUMNS}) \
         VALUES (?, ?, ?, ?, ?, NULL) \
         RETURNING {AUTH_SESSION_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(refresh_token_id)
    .bind(now.to_rfc3339())
    .bind(expires_at.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<AuthSession>, sqlx::Error> {
    let row: Option<AuthSession> = sqlx::query_as(&format!(
        "SELECT {AUTH_SESSION_COLUMNS} FROM auth_sessions WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn find_by_refresh_token_id(
    pool: &SqlitePool,
    refresh_token_id: &str,
) -> Result<Option<AuthSession>, sqlx::Error> {
    let row: Option<AuthSession> = sqlx::query_as(&format!(
        "SELECT {AUTH_SESSION_COLUMNS} FROM auth_sessions WHERE refresh_token_id = ?"
    ))
    .bind(refresh_token_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Rotate the session's current refresh token id (refresh-token rotation on
/// `/v1/tokens/refresh`). Returns false if the session row no longer exists.
pub async fn rotate_refresh_token(
    pool: &SqlitePool,
    id: Uuid,
    new_refresh_token_id: &str,
) -> Result<bool, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let res =
        sqlx::query("UPDATE auth_sessions SET refresh_token_id = ?, last_used_at = ? WHERE id = ?")
            .bind(new_refresh_token_id)
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn touch(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE auth_sessions SET last_used_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Invalidate (delete) a session. Returns true if a row was removed.
pub async fn invalidate(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM auth_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
