use api_types::{CreateIssueTagRequest, IssueTag};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct IssueTagRow {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub tag_id: Uuid,
    pub created_at: String,
}

impl IssueTagRow {
    pub fn into_api(self) -> IssueTag {
        IssueTag {
            id: self.id,
            issue_id: self.issue_id,
            tag_id: self.tag_id,
        }
    }
}

const SELECT: &str = r#"SELECT id, issue_id, tag_id,
 created_at FROM issue_tags"#;

pub async fn list_issue_tags(
    pool: &SqlitePool,
    issue_id: Uuid,
) -> Result<Vec<IssueTag>, sqlx::Error> {
    let rows: Vec<IssueTagRow> = sqlx::query_as(&format!(
        "{} WHERE issue_id = ? ORDER BY created_at",
        SELECT
    ))
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_api()).collect())
}

pub async fn get_issue_tag(pool: &SqlitePool, id: Uuid) -> Result<Option<IssueTag>, sqlx::Error> {
    let row: Option<IssueTagRow> = sqlx::query_as(&format!("{} WHERE id = ?", SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_api()))
}

pub async fn create_issue_tag(
    pool: &SqlitePool,
    req: &CreateIssueTagRequest,
) -> Result<IssueTag, sqlx::Error> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    let row: IssueTagRow = sqlx::query_as(
        r#"INSERT INTO issue_tags (id, issue_id, tag_id)
           VALUES (?,?,?) ON CONFLICT(issue_id, tag_id) DO NOTHING
           RETURNING id, issue_id, tag_id,
             created_at"#,
    )
    .bind(id)
    .bind(req.issue_id)
    .bind(req.tag_id)
    .fetch_optional(pool)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    Ok(row.into_api())
}

pub async fn delete_issue_tag(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM issue_tags WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
