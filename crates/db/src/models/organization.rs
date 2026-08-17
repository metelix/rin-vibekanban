use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub is_personal: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Ensure a personal organization exists for the user and that the user is an
/// admin member. Returns the organization id.
pub async fn ensure_personal_org_and_admin_membership(
    pool: &SqlitePool,
    user_id: Uuid,
    username: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    let name = username.unwrap_or("Personal").to_string();
    let slug = format!("user-{}", user_id.as_hyphenated());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO organizations (id, name, slug, is_personal, created_at, updated_at) \
         VALUES (?, ?, ?, 1, ?, ?) \
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(&name)
    .bind(&slug)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    let org_id: (Uuid,) = sqlx::query_as("SELECT id FROM organizations WHERE slug = ?")
        .bind(&slug)
        .fetch_one(pool)
        .await?;

    sqlx::query(
        "INSERT INTO organization_members (organization_id, user_id, role, created_at) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (organization_id, user_id) DO NOTHING",
    )
    .bind(org_id.0)
    .bind(user_id)
    .bind("admin")
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(org_id.0)
}
