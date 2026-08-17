use api_types::{
    CreateIssueRelationshipRequest, IssueRelationship, ListIssueRelationshipsQuery,
    ListIssueRelationshipsResponse, MutationResponse,
};
use axum::{
    Router,
    extract::{Json, Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::issue_relationship as issue_relationship_model;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};
use deployment::Deployment;

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/issue-relationships",
            get(list_issue_relationships).post(create_issue_relationship),
        )
        .route(
            "/issue-relationships/{relationship_id}",
            axum::routing::delete(delete_issue_relationship),
        )
}

async fn list_issue_relationships(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListIssueRelationshipsQuery>,
) -> Result<ResponseJson<ApiResponse<ListIssueRelationshipsResponse>>, ApiError> {
    let relationships =
        issue_relationship_model::list_issue_relationships(&deployment.db().pool, query.issue_id)
            .await?;
    Ok(ResponseJson(ApiResponse::success(
        ListIssueRelationshipsResponse {
            issue_relationships: relationships,
        },
    )))
}

async fn create_issue_relationship(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateIssueRelationshipRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<IssueRelationship>>>, ApiError> {
    let relationship =
        issue_relationship_model::create_issue_relationship(&deployment.db().pool, &request)
            .await?;
    Ok(ResponseJson(ApiResponse::success(MutationResponse {
        data: relationship,
        txid: 0,
    })))
}

async fn delete_issue_relationship(
    State(deployment): State<DeploymentImpl>,
    Path(relationship_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let deleted =
        issue_relationship_model::delete_issue_relationship(&deployment.db().pool, relationship_id)
            .await?;
    if !deleted {
        return Err(ApiError::Database(sqlx::Error::RowNotFound));
    }
    Ok(ResponseJson(ApiResponse::success(())))
}
