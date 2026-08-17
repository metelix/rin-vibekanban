use api_types::{ListProjectsQuery, ListProjectsResponse, Project as ApiProject};
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use db::models::project::Project as DbProject;
use uuid::Uuid;

use super::{V1State, middleware::RequestContext};

#[derive(Debug, thiserror::Error)]
pub(super) enum ProjectsError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for ProjectsError {
    fn into_response(self) -> Response {
        match self {
            ProjectsError::Database(err) => {
                tracing::error!(error = %err, "database error in /v1/projects");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "internal_error" })),
                )
                    .into_response()
            }
        }
    }
}

/// List all local projects belonging to the given (personal) organization.
/// For the self-hosted single-org deployment every local project is treated as
/// belonging to the caller's personal org, so we return all rows.
pub(super) async fn list_projects(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Json<ListProjectsResponse>, ProjectsError> {
    let projects = DbProject::find_all(&state.pool)
        .await?
        .into_iter()
        .map(|project| to_api_project(project, query.organization_id))
        .collect();

    Ok(Json(ListProjectsResponse { projects }))
}

/// Minimal create: insert a local project row and return it mapped to the API
/// shape. The color/sort_order fields don't exist on the local table, so they
/// use the same defaults as the `to_api_project` mapping.
pub(super) async fn create_project(
    State(state): State<V1State>,
    Extension(_ctx): Extension<RequestContext>,
    Json(payload): Json<api_types::CreateProjectRequest>,
) -> Result<Json<ApiProject>, ProjectsError> {
    let id = payload.id.unwrap_or_else(Uuid::new_v4);
    let inserted = DbProject::insert(&state.pool, id, &payload.name).await?;

    Ok(Json(to_api_project(inserted, payload.organization_id)))
}

/// Map a local project to the remote (cloud) API shape. The local `projects`
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
