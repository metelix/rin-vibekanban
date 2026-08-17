use api_types::Workspace as ApiWorkspace;
use axum::{
    Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::workspace::Workspace as DbWorkspace;
use deployment::Deployment;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new().route(
        "/workspaces/by-local-id/{local_workspace_id}",
        get(get_workspace_by_local_id),
    )
}

async fn get_workspace_by_local_id(
    State(deployment): State<DeploymentImpl>,
    Path(local_workspace_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<ApiWorkspace>>, ApiError> {
    let workspace = DbWorkspace::find_by_id(&deployment.db().pool, local_workspace_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    // The remote API workspace shape carries cloud-only metadata (project,
    // owner, diffs). The local workspaces table has none of that, so fill it
    // with a representative subset and sane defaults for self-hosted use.
    let api = ApiWorkspace {
        id: workspace.id,
        project_id: Uuid::nil(),
        owner_user_id: Uuid::nil(),
        issue_id: None,
        local_workspace_id: Some(workspace.id),
        name: workspace.name,
        archived: workspace.archived,
        files_changed: None,
        lines_added: None,
        lines_removed: None,
        created_at: workspace.created_at,
        updated_at: workspace.updated_at,
    };
    Ok(ResponseJson(ApiResponse::success(api)))
}
