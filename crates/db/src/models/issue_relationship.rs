use api_types::{CreateIssueRelationshipRequest, IssueRelationship, IssueRelationshipType};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct IssueRelationshipRow {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub related_issue_id: Uuid,
    pub relationship_type: String,
    pub created_at: String,
}

impl IssueRelationshipRow {
    pub fn into_api(self) -> IssueRelationship {
        let relationship_type = match self.relationship_type.as_str() {
            "blocking" => IssueRelationshipType::Blocking,
            "related" => IssueRelationshipType::Related,
            _ => IssueRelationshipType::HasDuplicate,
        };
        IssueRelationship {
            id: self.id,
            issue_id: self.issue_id,
            related_issue_id: self.related_issue_id,
            relationship_type,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)
                .ok()
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        }
    }
}

fn rel_type_to_sql(t: IssueRelationshipType) -> &'static str {
    match t {
        IssueRelationshipType::Blocking => "blocking",
        IssueRelationshipType::Related => "related",
        IssueRelationshipType::HasDuplicate => "has_duplicate",
    }
}

const SELECT: &str = r#"SELECT id as "id!: Uuid", issue_id as "issue_id!: Uuid",
 related_issue_id as "related_issue_id!: Uuid", relationship_type, created_at as "created_at!"
 FROM issue_relationships"#;

pub async fn list_issue_relationships(
    pool: &SqlitePool,
    issue_id: Uuid,
) -> Result<Vec<IssueRelationship>, sqlx::Error> {
    let rows: Vec<IssueRelationshipRow> = sqlx::query_as(&format!(
        "{} WHERE issue_id = ? ORDER BY created_at",
        SELECT
    ))
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_api()).collect())
}

pub async fn create_issue_relationship(
    pool: &SqlitePool,
    req: &CreateIssueRelationshipRequest,
) -> Result<IssueRelationship, sqlx::Error> {
    let id = req.id.unwrap_or_else(Uuid::new_v4);
    let row: IssueRelationshipRow = sqlx::query_as(
        r#"INSERT INTO issue_relationships (id, issue_id, related_issue_id, relationship_type)
           VALUES (?,?,?,?)
           RETURNING id as "id!: Uuid", issue_id as "issue_id!: Uuid",
             related_issue_id as "related_issue_id!: Uuid", relationship_type, created_at as "created_at!""#,
    )
    .bind(id)
    .bind(req.issue_id)
    .bind(req.related_issue_id)
    .bind(rel_type_to_sql(req.relationship_type))
    .fetch_one(pool)
    .await?;
    Ok(row.into_api())
}

pub async fn delete_issue_relationship(pool: &SqlitePool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM issue_relationships WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
