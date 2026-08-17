use api_types::{
    GetOrganizationResponse, ListOrganizationsResponse, MemberRole, Organization,
    OrganizationWithRole,
};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::{V1State, middleware::RequestContext};

/// DB row for an organization joined with the caller's membership role.
/// Uses runtime `query_as` + plain column names (NO macro-aliases) so that
/// sqlx decodes BLOB UUIDs and stores booleans as INTEGER correctly.
#[derive(Debug, Clone, FromRow)]
struct DbOrgWithRole {
    id: Uuid,
    name: String,
    slug: String,
    is_personal: i64,
    created_at: String,
    updated_at: String,
    role: String,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum OrgError {
    #[error("organization not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for OrgError {
    fn into_response(self) -> Response {
        match self {
            OrgError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "not_found" })),
            )
                .into_response(),
            OrgError::Database(err) => {
                tracing::error!(error = %err, "database error in /v1/organizations");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal_error" })),
                )
                    .into_response()
            }
        }
    }
}

const ORG_WITH_ROLE_SELECT: &str = "SELECT o.id, o.name, o.slug, o.is_personal, \
     o.created_at, o.updated_at, om.role \
     FROM organizations o \
     JOIN organization_members om ON om.organization_id = o.id";

fn parse_ts(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Map the lowercase role text stored in `organization_members.role` to the
/// api-types `MemberRole`. Self-hosted personal orgs store `admin`.
fn role_to_member_role(role: &str) -> MemberRole {
    match role.to_ascii_lowercase().as_str() {
        "admin" | "owner" => MemberRole::Admin,
        _ => MemberRole::Member,
    }
}

/// The `is_personal` column is stored as INTEGER in SQLite; convert to bool.
fn is_personal(flag: i64) -> bool {
    flag != 0
}

pub(super) async fn list_organizations(
    State(state): State<V1State>,
    Extension(ctx): Extension<RequestContext>,
) -> Result<Json<ListOrganizationsResponse>, OrgError> {
    let rows: Vec<DbOrgWithRole> =
        sqlx::query_as(&format!("{ORG_WITH_ROLE_SELECT} WHERE om.user_id = ?"))
            .bind(ctx.user_id)
            .fetch_all(&state.pool)
            .await?;

    let organizations = rows
        .into_iter()
        .map(|row| OrganizationWithRole {
            id: row.id,
            name: row.name,
            slug: row.slug,
            is_personal: is_personal(row.is_personal),
            issue_prefix: String::new(),
            created_at: parse_ts(&row.created_at),
            updated_at: parse_ts(&row.updated_at),
            user_role: role_to_member_role(&row.role),
        })
        .collect();

    Ok(Json(ListOrganizationsResponse { organizations }))
}

pub(super) async fn get_organization(
    State(state): State<V1State>,
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<GetOrganizationResponse>, OrgError> {
    let row: Option<DbOrgWithRole> = sqlx::query_as(&format!(
        "{ORG_WITH_ROLE_SELECT} WHERE o.id = ? AND om.user_id = ?"
    ))
    .bind(org_id)
    .bind(ctx.user_id)
    .fetch_optional(&state.pool)
    .await?;

    let row = row.ok_or(OrgError::NotFound)?;

    let organization = Organization {
        id: row.id,
        name: row.name,
        slug: row.slug,
        is_personal: is_personal(row.is_personal),
        issue_prefix: String::new(),
        created_at: parse_ts(&row.created_at),
        updated_at: parse_ts(&row.updated_at),
    };

    // Serialize `MemberRole` through its canonical SCREAMING_SNAKE_CASE form
    // (e.g. "ADMIN") so it matches the enum's JSON representation.
    let user_role = serde_json::to_value(role_to_member_role(&row.role))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "MEMBER".to_string());

    Ok(Json(GetOrganizationResponse {
        organization,
        user_role,
    }))
}
