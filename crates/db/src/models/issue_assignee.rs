use api_types::{CreateIssueAssigneeRequest, IssueAssignee};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct IssueAssigneeRow {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub user_id: Uuid,
    pub assigned_at: String,
}

impl IssueAssigneeRow {
    pub fn into_api(self) -> IssueAssignee {
        IssueAssignee {
            id: self.id,
            issue_id: self.issue_id,
            user_id: self.user_id,
            assigned_at: DateTime::parse_from_rfc3339(&self.assigned_at)
                .ok()
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        }
    }
}

const SELECT: &str = r#"SELECT id, issue_id, user_id,
 assigned_at FROM issue_assignees"#;

pub async fn list_issue_assignees(
    pool: &SqlitePool,
    issue_id: Uuid,
) -> Result<Vec<IssueAssignee>, sqlx::Error> {
    let rows: Vec<IssueAssigneeRow> = sqlx::query_as(&format!(
        "{} WHERE issue_id = ? ORDER BY assigned_at",
        SELECT
    ))
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_api()).collect())
}

/// List assignees for every issue in a project (join on `issues.project_id`).
pub async fn list_issue_assignees_for_project(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<Vec<IssueAssignee>, sqlx::Error> {
    let rows: Vec<IssueAssigneeRow> = sqlx::query_as(&format!(
        "{SELECT} JOIN issues i ON i.id = issue_assignees.issue_id \
         WHERE i.project_id = ? ORDER BY issue_assignees.assigned_at"
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_api()).collect())
}

pub async fn get_issue_assignee(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Option<IssueAssignee>, sqlx::Error> {
    let row: Option<IssueAssigneeRow> = sqlx::query_as(&format!("{} WHERE id = ?", SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_api()))
}

pub async fn create_issue_assignee(
    pool: &SqlitePool,
    req: &CreateIssueAssigneeRequest,
) -> Result<IssueAssignee, sqlx::Error> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    let row: IssueAssigneeRow = sqlx::query_as(
        r#"INSERT INTO issue_assignees (id, issue_id, user_id)
           VALUES (?,?,?) ON CONFLICT(issue_id, user_id) DO NOTHING
           RETURNING id, issue_id, user_id,
             assigned_at"#,
    )
    .bind(id)
    .bind(req.issue_id)
    .bind(req.user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    Ok(row.into_api())
}

pub async fn delete_issue_assignee(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM issue_assignees WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
