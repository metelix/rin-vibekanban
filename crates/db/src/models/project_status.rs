use api_types::{CreateProjectStatusRequest, ProjectStatus, UpdateProjectStatusRequest};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ProjectStatusRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub hidden: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl ProjectStatusRow {
    pub fn into_api(self) -> ProjectStatus {
        ProjectStatus {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            color: self.color,
            sort_order: self.sort_order as i32,
            hidden: self.hidden != 0,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .ok()
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        }
    }
}

const SELECT: &str = r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", name, color,
 sort_order as "sort_order!: i64", hidden as "hidden!: i64", created_at as "created_at!", updated_at as "updated_at!"
 FROM project_statuses"#;

pub async fn list_project_statuses(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<Vec<ProjectStatus>, sqlx::Error> {
    let rows: Vec<ProjectStatusRow> = sqlx::query_as(&format!(
        "{} WHERE project_id = ? ORDER BY sort_order",
        SELECT
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_api()).collect())
}

pub async fn get_project_status(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Option<ProjectStatus>, sqlx::Error> {
    let row: Option<ProjectStatusRow> = sqlx::query_as(&format!("{} WHERE id = ?", SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_api()))
}

pub async fn create_project_status(
    pool: &SqlitePool,
    req: &CreateProjectStatusRequest,
) -> Result<ProjectStatus, sqlx::Error> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    let now = Utc::now().to_rfc3339();
    let row: ProjectStatusRow = sqlx::query_as(
        r#"INSERT INTO project_statuses (id, project_id, name, color, sort_order, hidden, created_at, updated_at)
           VALUES (?,?,?,?,?,?,?,?)
           RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", name, color,
             sort_order as "sort_order!: i64", hidden as "hidden!: i64", created_at as "created_at!", updated_at as "updated_at!""#,
    )
    .bind(id)
    .bind(req.project_id)
    .bind(&req.name)
    .bind(&req.color)
    .bind(req.sort_order)
    .bind(req.hidden)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(row.into_api())
}

pub async fn update_project_status(
    pool: &SqlitePool,
    id: Uuid,
    req: &UpdateProjectStatusRequest,
) -> Result<Option<ProjectStatus>, sqlx::Error> {
    let existing = match get_project_status(pool, id).await? {
        Some(s) => s,
        None => return Ok(None),
    };
    let name = req.name.clone().unwrap_or(existing.name);
    let color = req.color.clone().unwrap_or(existing.color);
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let hidden = req.hidden.unwrap_or(existing.hidden);
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE project_statuses SET name=?, color=?, sort_order=?, hidden=?, updated_at=? WHERE id=?",
    )
    .bind(&name)
    .bind(&color)
    .bind(sort_order)
    .bind(hidden)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    get_project_status(pool, id).await
}

pub async fn delete_project_status(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM project_statuses WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Ensure a project has the default status set (Todo, In Progress, Done, etc).
pub async fn ensure_default_statuses(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<(), sqlx::Error> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM project_statuses WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    if count.0 > 0 {
        return Ok(());
    }
    let defaults = [
        ("todo", "#3b82f6", 0),
        ("inprogress", "#f59e0b", 10),
        ("done", "#10b981", 20),
        ("cancelled", "#6b7280", 30),
        ("inreview", "#8b5cf6", 40),
    ];
    for (name, color, pos) in defaults {
        let row: ProjectStatusRow = sqlx::query_as(
            r#"INSERT INTO project_statuses (id, project_id, name, color, sort_order, hidden, created_at, updated_at)
               VALUES (?,?,?,?,?,0,datetime('now'),datetime('now'))
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", name, color,
                 sort_order as "sort_order!: i64", hidden as "hidden!: i64", created_at as "created_at!", updated_at as "updated_at!""#,
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(name)
        .bind(color)
        .bind(pos)
        .fetch_one(pool)
        .await?;
        let _ = row;
    }
    Ok(())
}
