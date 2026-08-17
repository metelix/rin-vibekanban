use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const USER_COLUMNS: &str = "id, email, username, first_name, last_name, created_at, updated_at";

/// Insert a user for the given email, or no-op (returning the existing row) on conflict.
/// Returns the user row once persisted.
pub async fn upsert_user(
    pool: &SqlitePool,
    email: &str,
    username: Option<&str>,
) -> Result<User, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let row: User = sqlx::query_as(&format!(
        "INSERT INTO users ({USER_COLUMNS}) VALUES (?, ?, ?, NULL, NULL, ?, ?) \
         ON CONFLICT (email) DO UPDATE SET updated_at = excluded.updated_at \
         RETURNING {USER_COLUMNS}"
    ))
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(username)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn find_user_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    let row: Option<User> =
        sqlx::query_as(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?"))
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

pub async fn find_user_by_email(
    pool: &SqlitePool,
    email: &str,
) -> Result<Option<User>, sqlx::Error> {
    let row: Option<User> =
        sqlx::query_as(&format!("SELECT {USER_COLUMNS} FROM users WHERE email = ?"))
            .bind(email)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}
