use api_types::{
    BulkUpdateIssuesRequest, BulkUpdateIssuesResponse, BulkUpdateProjectStatusesRequest,
};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use db::models::{
    issue as db_issue, issue_assignee as db_issue_assignee,
    issue_relationship as db_issue_relationship, issue_tag as db_issue_tag,
    kanban_tag as db_kanban_tag, project as db_project, project_status as db_status,
};
use uuid::Uuid;

use super::{V1State, middleware::RequestContext};

#[derive(Debug, thiserror::Error)]
pub(super) enum KanbanError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for KanbanError {
    fn into_response(self) -> Response {
        match self {
            KanbanError::Database(err) => {
                tracing::error!(error = %err, "database error in /v1 kanban routes");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal_error" })),
                )
                    .into_response()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Read model: raw JSON arrays of api_types rows, keyed by project.
// These replace the ElectricSQL shape reads (`PROJECT_ISSUES_SHAPE`, etc.).
// ---------------------------------------------------------------------------

pub(super) async fn list_issues(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<api_types::Issue>>, KanbanError> {
    let (issues, _total) = db_issue::list_issues(&state.pool, project_id).await?;
    Ok(Json(issues))
}

pub(super) async fn list_statuses(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<api_types::ProjectStatus>>, KanbanError> {
    let statuses = db_status::list_project_statuses(&state.pool, project_id).await?;
    Ok(Json(statuses))
}

pub(super) async fn list_assignees(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<api_types::IssueAssignee>>, KanbanError> {
    let assignees =
        db_issue_assignee::list_issue_assignees_for_project(&state.pool, project_id).await?;
    Ok(Json(assignees))
}

pub(super) async fn list_tags(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<api_types::Tag>>, KanbanError> {
    let tags = db_kanban_tag::list_tags(&state.pool, project_id).await?;
    Ok(Json(tags))
}

pub(super) async fn list_relationships(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<api_types::IssueRelationship>>, KanbanError> {
    let relationships =
        db_issue_relationship::list_issue_relationships_for_project(&state.pool, project_id)
            .await?;
    Ok(Json(relationships))
}

pub(super) async fn list_issue_tags(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<api_types::IssueTag>>, KanbanError> {
    let tags = db_issue_tag::list_issue_tags_for_project(&state.pool, project_id).await?;
    Ok(Json(tags))
}

/// No `issue_comments` table/model exists in the local backend yet, so this
/// returns an empty array. Kept as a stable endpoint so the frontend doesn't
/// need a conditional code path before comments are implemented.
pub(super) async fn list_comments(
    State(_state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Path(_project_id): Path<Uuid>,
) -> Json<Vec<serde_json::Value>> {
    Json(Vec::new())
}

// ---------------------------------------------------------------------------
// Write model: batch endpoints consumed by remote-web's remoteApi.ts.
// Each client sends `{ "updates": [{ "id": ..., ...changes }] }` and only
// checks for a 2xx status; we return the affected api_types rows anyway.
// ---------------------------------------------------------------------------

pub(super) async fn bulk_update_issues(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Json(payload): Json<BulkUpdateIssuesRequest>,
) -> Result<Json<BulkUpdateIssuesResponse>, KanbanError> {
    let mut data = Vec::with_capacity(payload.updates.len());
    for item in payload.updates {
        if let Some(issue) = db_issue::update_issue(&state.pool, item.id, &item.changes).await? {
            data.push(issue);
        }
    }
    Ok(Json(BulkUpdateIssuesResponse { data, txid: 0 }))
}

pub(super) async fn bulk_update_project_statuses(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Json(payload): Json<BulkUpdateProjectStatusesRequest>,
) -> Result<Json<api_types::BulkUpdateProjectStatusesResponse>, KanbanError> {
    let mut data = Vec::with_capacity(payload.updates.len());
    for item in payload.updates {
        if let Some(status) =
            db_status::update_project_status(&state.pool, item.id, &item.changes).await?
        {
            data.push(status);
        }
    }
    Ok(Json(api_types::BulkUpdateProjectStatusesResponse {
        data,
        txid: 0,
    }))
}

pub(super) async fn bulk_update_projects(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Json(payload): Json<api_types::BulkUpdateProjectsRequest>,
) -> Result<Json<api_types::BulkUpdateProjectsResponse>, KanbanError> {
    let mut data = Vec::with_capacity(payload.updates.len());
    for item in payload.updates {
        if let Some(project) =
            db_project::Project::update(&state.pool, item.id, &item.changes).await?
        {
            data.push(to_api_project(project));
        }
    }
    Ok(Json(api_types::BulkUpdateProjectsResponse {
        data,
        txid: 0,
    }))
}

/// Map a local project row to the API shape. The local `projects` table has
/// no organization/color/sort_order columns, so those use self-hosted defaults.
fn to_api_project(project: db_project::Project) -> api_types::Project {
    api_types::Project {
        id: project.id,
        organization_id: Uuid::nil(),
        name: project.name,
        color: String::new(),
        sort_order: 0,
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}
