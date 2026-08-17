use api_types::{CreateTagRequest, Tag, UpdateTagRequest};
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct KanbanTagRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

impl KanbanTagRow {
    pub fn into_api(self) -> Tag {
        Tag {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            color: self.color,
        }
    }
}

const SELECT: &str = r#"SELECT id, project_id, name, color,
 created_at, updated_at FROM kanban_tags"#;

pub async fn list_tags(pool: &SqlitePool, project_id: Uuid) -> Result<Vec<Tag>, sqlx::Error> {
    let rows: Vec<KanbanTagRow> =
        sqlx::query_as(&format!("{} WHERE project_id = ? ORDER BY name", SELECT))
            .bind(project_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.into_api()).collect())
}

pub async fn get_tag(pool: &SqlitePool, id: Uuid) -> Result<Option<Tag>, sqlx::Error> {
    let row: Option<KanbanTagRow> = sqlx::query_as(&format!("{} WHERE id = ?", SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_api()))
}

pub async fn create_tag(pool: &SqlitePool, req: &CreateTagRequest) -> Result<Tag, sqlx::Error> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    let now = Utc::now().to_rfc3339();
    let row: KanbanTagRow = sqlx::query_as(
        r#"INSERT INTO kanban_tags (id, project_id, name, color, created_at, updated_at)
           VALUES (?,?,?,?,?,?)
           RETURNING id, project_id, name, color,
             created_at, updated_at"#,
    )
    .bind(id)
    .bind(req.project_id)
    .bind(&req.name)
    .bind(&req.color)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(row.into_api())
}

pub async fn update_tag(
    pool: &SqlitePool,
    id: Uuid,
    req: &UpdateTagRequest,
) -> Result<Option<Tag>, sqlx::Error> {
    let existing = match get_tag(pool, id).await? {
        Some(t) => t,
        None => return Ok(None),
    };
    let name = req.name.clone().unwrap_or(existing.name);
    let color = req.color.clone().unwrap_or(existing.color);
    let now = Utc::now().to_rfc3339();

    sqlx::query("UPDATE kanban_tags SET name=?, color=?, updated_at=? WHERE id=?")
        .bind(&name)
        .bind(&color)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    get_tag(pool, id).await
}

pub async fn delete_tag(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM kanban_tags WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
