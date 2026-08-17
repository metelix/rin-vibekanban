use api_types::UpdateProjectRequest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub default_agent_working_dir: Option<String>,
    pub remote_project_id: Option<Uuid>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub async fn find_all(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Project,
            r#"SELECT id as "id!: Uuid",
                      name,
                      default_agent_working_dir,
                      remote_project_id as "remote_project_id: Uuid",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM projects
               ORDER BY created_at DESC"#
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            "SELECT id, name, default_agent_working_dir, remote_project_id, created_at, updated_at
             FROM projects
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Insert a new local project with the given id and name. Timestamps are
    /// written in RFC3339. Returns the persisted row.
    pub async fn insert(pool: &SqlitePool, id: Uuid, name: &str) -> Result<Self, sqlx::Error> {
        use chrono::Utc;
        let now = Utc::now().to_rfc3339();
        let row: Self = sqlx::query_as::<_, Project>(
            "INSERT INTO projects (id, name, created_at, updated_at) \
             VALUES (?, ?, ?, ?) \
             RETURNING id, name, default_agent_working_dir, remote_project_id, created_at, updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(&now)
        .bind(&now)
        .fetch_one(pool)
        .await?;
        Ok(row)
    }

    pub async fn set_remote_project_id(
        pool: &SqlitePool,
        id: Uuid,
        remote_project_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE projects SET remote_project_id = ? WHERE id = ?")
            .bind(remote_project_id)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Apply a partial update to a local project. The self-hosted `projects`
    /// table only has a `name` column (no color/sort_order), so only `name`
    /// is written; color/sort_order in the request are ignored. Returns the
    /// updated row, or `None` if the project does not exist.
    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        req: &UpdateProjectRequest,
    ) -> Result<Option<Self>, sqlx::Error> {
        let existing = match Self::find_by_id(pool, id).await? {
            Some(p) => p,
            None => return Ok(None),
        };
        let name = req.name.clone().unwrap_or(existing.name);
        let now = Utc::now().to_rfc3339();

        sqlx::query("UPDATE projects SET name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;

        Self::find_by_id(pool, id).await
    }
}
