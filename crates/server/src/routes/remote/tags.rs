use api_types::{ListTagsResponse, Tag};
use axum::{
    Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::kanban_tag as kanban_tag_model;
use serde::Deserialize;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};
use deployment::Deployment;

#[derive(Debug, Deserialize)]
pub(super) struct ListTagsQuery {
    pub project_id: Uuid,
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/tags", get(list_tags))
        .route("/tags/{tag_id}", get(get_tag))
}

async fn list_tags(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListTagsQuery>,
) -> Result<ResponseJson<ApiResponse<ListTagsResponse>>, ApiError> {
    let tags = kanban_tag_model::list_tags(&deployment.db().pool, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(ListTagsResponse {
        tags,
    })))
}

async fn get_tag(
    State(deployment): State<DeploymentImpl>,
    Path(tag_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Tag>>, ApiError> {
    let tag = kanban_tag_model::get_tag(&deployment.db().pool, tag_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;
    Ok(ResponseJson(ApiResponse::success(tag)))
}
