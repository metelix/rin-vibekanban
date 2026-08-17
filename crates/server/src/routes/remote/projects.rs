use api_types::{ListProjectsResponse, Project as ApiProject};
use axum::{
    Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::project::Project as DbProject;
use deployment::Deployment;
use serde::Deserialize;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct ListRemoteProjectsQuery {
    /// Optional org filter. When omitted (allowed so MCP `list_projects`
    /// works without an org), projects are returned with a nil org id —
    /// appropriate for self-hosted single-org deployment.
    pub organization_id: Option<Uuid>,
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/projects", get(list_remote_projects))
        .route("/projects/{project_id}", get(get_remote_project))
}

async fn list_remote_projects(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListRemoteProjectsQuery>,
) -> Result<ResponseJson<ApiResponse<ListProjectsResponse>>, ApiError> {
    let org_id = query.organization_id.unwrap_or_else(Uuid::nil);
    let projects = DbProject::find_all(&deployment.db().pool)
        .await?
        .into_iter()
        .map(|project| to_api_project(project, org_id))
        .collect();
    Ok(ResponseJson(ApiResponse::success(ListProjectsResponse {
        projects,
    })))
}

async fn get_remote_project(
    State(deployment): State<DeploymentImpl>,
    Path(project_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<ApiProject>>, ApiError> {
    let project = DbProject::find_by_id(&deployment.db().pool, project_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;
    Ok(ResponseJson(ApiResponse::success(to_api_project(
        project,
        Uuid::nil(),
    ))))
}

/// Map a local project to the remote (cloud) API shape. The local projects
/// table has no organization/color/sort_order columns, so those fields use
/// sane defaults for self-hosted deployments.
fn to_api_project(project: DbProject, organization_id: Uuid) -> ApiProject {
    ApiProject {
        id: project.id,
        organization_id,
        name: project.name,
        color: String::new(),
        sort_order: 0,
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}
