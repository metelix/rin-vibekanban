use api_types::ListProjectStatusesResponse;
use axum::{
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
    Router,
};
use db::models::project_status as project_status_model;
use deployment::Deployment;
use serde::Deserialize;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{error::ApiError, DeploymentImpl};

#[derive(Debug, Deserialize)]
pub(super) struct ListProjectStatusesQuery {
    pub project_id: Uuid,
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new().route("/project-statuses", get(list_project_statuses))
}

async fn list_project_statuses(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListProjectStatusesQuery>,
) -> Result<ResponseJson<ApiResponse<ListProjectStatusesResponse>>, ApiError> {
    let statuses =
        project_status_model::list_project_statuses(&deployment.db().pool, query.project_id)
            .await?;
    Ok(ResponseJson(ApiResponse::success(
        ListProjectStatusesResponse {
            project_statuses: statuses,
        },
    )))
}
