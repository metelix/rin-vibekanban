use api_types::{
    CreateIssueAssigneeRequest, IssueAssignee, ListIssueAssigneesResponse, MutationResponse,
};
use axum::{
    Router,
    extract::{Json, Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::issue_assignee as issue_assignee_model;
use serde::Deserialize;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};
use deployment::Deployment;

#[derive(Debug, Deserialize)]
pub(super) struct ListIssueAssigneesQuery {
    pub issue_id: Uuid,
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/issue-assignees",
            get(list_issue_assignees).post(create_issue_assignee),
        )
        .route(
            "/issue-assignees/{issue_assignee_id}",
            get(get_issue_assignee).delete(delete_issue_assignee),
        )
}

async fn list_issue_assignees(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListIssueAssigneesQuery>,
) -> Result<ResponseJson<ApiResponse<ListIssueAssigneesResponse>>, ApiError> {
    let assignees =
        issue_assignee_model::list_issue_assignees(&deployment.db().pool, query.issue_id).await?;
    Ok(ResponseJson(ApiResponse::success(
        ListIssueAssigneesResponse {
            issue_assignees: assignees,
        },
    )))
}

async fn get_issue_assignee(
    State(deployment): State<DeploymentImpl>,
    Path(issue_assignee_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<IssueAssignee>>, ApiError> {
    let assignee =
        issue_assignee_model::get_issue_assignee(&deployment.db().pool, issue_assignee_id)
            .await?
            .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;
    Ok(ResponseJson(ApiResponse::success(assignee)))
}

async fn create_issue_assignee(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateIssueAssigneeRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<IssueAssignee>>>, ApiError> {
    let assignee =
        issue_assignee_model::create_issue_assignee(&deployment.db().pool, &request).await?;
    Ok(ResponseJson(ApiResponse::success(MutationResponse {
        data: assignee,
        txid: 0,
    })))
}

async fn delete_issue_assignee(
    State(deployment): State<DeploymentImpl>,
    Path(issue_assignee_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let deleted =
        issue_assignee_model::delete_issue_assignee(&deployment.db().pool, issue_assignee_id)
            .await?;
    if !deleted {
        return Err(ApiError::Database(sqlx::Error::RowNotFound));
    }
    Ok(ResponseJson(ApiResponse::success(())))
}
