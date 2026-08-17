use api_types::{CreateIssueRequest, Issue, IssuePriority, UpdateIssueRequest};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct IssueRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub issue_number: i64,
    pub simple_id: String,
    pub status_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub completed_at: Option<String>,
    pub sort_order: f64,
    pub parent_issue_id: Option<Uuid>,
    pub parent_issue_sort_order: Option<f64>,
    pub extension_metadata: String,
    pub creator_user_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

fn parse_priority(s: &Option<String>) -> Option<IssuePriority> {
    match s.as_deref() {
        Some("urgent") => Some(IssuePriority::Urgent),
        Some("high") => Some(IssuePriority::High),
        Some("medium") => Some(IssuePriority::Medium),
        Some("low") => Some(IssuePriority::Low),
        _ => None,
    }
}

fn parse_dt(s: &Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref().and_then(|v| {
        DateTime::parse_from_rfc3339(v)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
}

impl IssueRow {
    pub fn into_issue(self) -> Issue {
        Issue {
            id: self.id,
            project_id: self.project_id,
            issue_number: self.issue_number as i32,
            simple_id: self.simple_id,
            status_id: self.status_id,
            title: self.title,
            description: self.description,
            priority: parse_priority(&self.priority),
            start_date: parse_dt(&self.start_date),
            target_date: parse_dt(&self.target_date),
            completed_at: parse_dt(&self.completed_at),
            sort_order: self.sort_order,
            parent_issue_id: self.parent_issue_id,
            parent_issue_sort_order: self.parent_issue_sort_order,
            extension_metadata: serde_json::from_str(&self.extension_metadata)
                .unwrap_or_else(|_| serde_json::json!({})),
            creator_user_id: self.creator_user_id,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .ok()
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            updated_at: DateTime::parse_from_rfc3339(&self.updated_at)
                .ok()
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        }
    }
}

fn priority_to_sql(p: Option<IssuePriority>) -> Option<String> {
    p.map(|v| match v {
        IssuePriority::Urgent => "urgent".to_string(),
        IssuePriority::High => "high".to_string(),
        IssuePriority::Medium => "medium".to_string(),
        IssuePriority::Low => "low".to_string(),
    })
}

const ISSUE_SELECT: &str = r#"SELECT id, project_id, issue_number,
 simple_id, status_id, title, description,
 priority, start_date, target_date,
 completed_at, sort_order, parent_issue_id,
 parent_issue_sort_order, extension_metadata, creator_user_id,
 created_at, updated_at FROM issues"#;

pub async fn allocate_issue_number(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let next: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(issue_number), 0) + 1 FROM issues WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(next.0)
}

pub async fn project_slug(pool: &SqlitePool, project_id: Uuid) -> Result<String, sqlx::Error> {
    let row: (String,) = sqlx::query_as("SELECT name FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn create_issue(
    pool: &SqlitePool,
    project_id: Uuid,
    req: &CreateIssueRequest,
) -> Result<Issue, sqlx::Error> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    let issue_number = allocate_issue_number(pool, project_id).await?;
    let slug = project_slug(pool, project_id).await?;
    let simple_id = format!("{}-{}", slug, issue_number);
    let status_id = req.status_id;
    let now = Utc::now().to_rfc3339();
    let meta = req.extension_metadata.to_string();

    let row: IssueRow = sqlx::query_as(
        r#"INSERT INTO issues (id, project_id, issue_number, simple_id, status_id, title, description,
           priority, start_date, target_date, completed_at, sort_order, parent_issue_id,
           parent_issue_sort_order, extension_metadata, creator_user_id, created_at, updated_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
           RETURNING id, project_id, issue_number,
             simple_id, status_id, title, description,
             priority, start_date, target_date,
             completed_at, sort_order, parent_issue_id,
             parent_issue_sort_order, extension_metadata, creator_user_id,
             created_at, updated_at"#,
    )
    .bind(id)
    .bind(project_id)
    .bind(issue_number)
    .bind(simple_id)
    .bind(status_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(priority_to_sql(req.priority))
    .bind(req.start_date.map(|d| d.to_rfc3339()))
    .bind(req.target_date.map(|d| d.to_rfc3339()))
    .bind(req.completed_at.map(|d| d.to_rfc3339()))
    .bind(req.sort_order)
    .bind(req.parent_issue_id)
    .bind(req.parent_issue_sort_order)
    .bind(meta)
    .bind(Option::<Uuid>::None)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;

    Ok(row.into_issue())
}

pub async fn get_issue(pool: &SqlitePool, id: Uuid) -> Result<Option<Issue>, sqlx::Error> {
    let row: Option<IssueRow> = sqlx::query_as(&format!("{} WHERE id = ?", ISSUE_SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_issue()))
}

pub async fn list_issues(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<(Vec<Issue>, usize), sqlx::Error> {
    let rows: Vec<IssueRow> = sqlx::query_as(&format!(
        "{} WHERE project_id = ? ORDER BY sort_order, created_at",
        ISSUE_SELECT
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let total = rows.len();
    Ok((rows.into_iter().map(|r| r.into_issue()).collect(), total))
}

pub async fn update_issue(
    pool: &SqlitePool,
    id: Uuid,
    req: &UpdateIssueRequest,
) -> Result<Option<Issue>, sqlx::Error> {
    let existing = match get_issue(pool, id).await? {
        Some(e) => e,
        None => return Ok(None),
    };

    let status_id = req.status_id.unwrap_or(existing.status_id);
    let title = req.title.clone().unwrap_or(existing.title);
    let description = req.description.clone().unwrap_or(existing.description);
    let priority = match req.priority {
        Some(p) => priority_to_sql(p),
        None => priority_to_sql(existing.priority),
    };
    let start_date = req
        .start_date
        .map(|d| d.map(|x| x.to_rfc3339()))
        .unwrap_or(existing.start_date.map(|d| d.to_rfc3339()));
    let target_date = req
        .target_date
        .map(|d| d.map(|x| x.to_rfc3339()))
        .unwrap_or(existing.target_date.map(|d| d.to_rfc3339()));
    let completed_at = req
        .completed_at
        .map(|d| d.map(|x| x.to_rfc3339()))
        .unwrap_or(existing.completed_at.map(|d| d.to_rfc3339()));
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let parent_issue_id = req.parent_issue_id.unwrap_or(existing.parent_issue_id);
    let parent_issue_sort_order = req
        .parent_issue_sort_order
        .unwrap_or(existing.parent_issue_sort_order);
    let extension_metadata = req
        .extension_metadata
        .clone()
        .unwrap_or(existing.extension_metadata)
        .to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"UPDATE issues SET status_id=?, title=?, description=?, priority=?, start_date=?, target_date=?,
           completed_at=?, sort_order=?, parent_issue_id=?, parent_issue_sort_order=?, extension_metadata=?, updated_at=?
           WHERE id=?"#,
    )
    .bind(status_id)
    .bind(&title)
    .bind(&description)
    .bind(&priority)
    .bind(&start_date)
    .bind(&target_date)
    .bind(&completed_at)
    .bind(sort_order)
    .bind(parent_issue_id)
    .bind(parent_issue_sort_order)
    .bind(&extension_metadata)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    get_issue(pool, id).await
}

pub async fn delete_issue(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM issues WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
