use api_types::{
    CreateIssueRequest, Issue, IssuePriority, IssueSortField, ListIssuesQuery, ListIssuesResponse,
    MutationResponse, SearchIssuesRequest, SortDirection, UpdateIssueRequest,
};
use axum::{
    Router,
    extract::{Json, Path, Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{issue as db_issue, project_status as db_status};
use deployment::Deployment;
use sqlx::SqlitePool;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/issues", get(list_issues).post(create_issue))
        .route("/issues/search", post(search_issues))
        .route(
            "/issues/{issue_id}",
            get(get_issue).patch(update_issue).delete(delete_issue),
        )
}

const DEFAULT_LIMIT: usize = 100;

async fn list_issues(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<ResponseJson<ApiResponse<ListIssuesResponse>>, ApiError> {
    let (issues, total) = db_issue::list_issues(&deployment.db().pool, query.project_id).await?;
    let response = ListIssuesResponse {
        issues,
        total_count: total,
        limit: DEFAULT_LIMIT,
        offset: 0,
    };
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn search_issues(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<SearchIssuesRequest>,
) -> Result<ResponseJson<ApiResponse<ListIssuesResponse>>, ApiError> {
    let (issues, _total) = db_issue::list_issues(&deployment.db().pool, request.project_id).await?;

    let mut issues: Vec<Issue> = issues
        .into_iter()
        .filter(|issue| {
            if let Some(status_id) = request.status_id {
                if issue.status_id != status_id {
                    return false;
                }
            }
            if let Some(status_ids) = &request.status_ids {
                if !status_ids.contains(&issue.status_id) {
                    return false;
                }
            }
            if let Some(priority) = request.priority {
                if issue.priority != Some(priority) {
                    return false;
                }
            }
            if let Some(parent_id) = request.parent_issue_id {
                if issue.parent_issue_id != Some(parent_id) {
                    return false;
                }
            }
            if let Some(simple_id) = &request.simple_id {
                if !issue
                    .simple_id
                    .to_lowercase()
                    .contains(&simple_id.to_lowercase())
                {
                    return false;
                }
            }
            if let Some(search) = &request.search {
                let needle = search.to_lowercase();
                let title_matches = issue.title.to_lowercase().contains(&needle);
                let desc_matches = issue
                    .description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&needle))
                    .unwrap_or(false);
                if !title_matches && !desc_matches {
                    return false;
                }
            }
            true
        })
        .collect();

    apply_sort(&mut issues, request.sort_field, request.sort_direction);

    let offset = request.offset.unwrap_or(0).max(0) as usize;
    let limit = if request.limit.unwrap_or(0) > 0 {
        request.limit.unwrap() as usize
    } else {
        DEFAULT_LIMIT
    };
    let total_count = issues.len();
    let issues: Vec<Issue> = issues.into_iter().skip(offset).take(limit).collect();

    let response = ListIssuesResponse {
        issues,
        total_count,
        limit,
        offset,
    };
    Ok(ResponseJson(ApiResponse::success(response)))
}

fn apply_sort(
    issues: &mut Vec<Issue>,
    sort_field: Option<IssueSortField>,
    sort_direction: Option<SortDirection>,
) {
    let field = sort_field.unwrap_or(IssueSortField::SortOrder);
    let dir = sort_direction.unwrap_or(SortDirection::Asc);

    let priority_rank = |p: &Option<IssuePriority>| match p {
        Some(IssuePriority::Urgent) => 0,
        Some(IssuePriority::High) => 1,
        Some(IssuePriority::Medium) => 2,
        Some(IssuePriority::Low) => 3,
        None => i32::MAX,
    };

    issues.sort_by(|a, b| {
        let cmp = match field {
            IssueSortField::SortOrder => a
                .sort_order
                .partial_cmp(&b.sort_order)
                .unwrap_or(std::cmp::Ordering::Equal),
            IssueSortField::Priority => priority_rank(&a.priority).cmp(&priority_rank(&b.priority)),
            IssueSortField::CreatedAt => a.created_at.cmp(&b.created_at),
            IssueSortField::UpdatedAt => a.updated_at.cmp(&b.updated_at),
            IssueSortField::Title => a.title.cmp(&b.title),
        };
        match dir {
            SortDirection::Asc => cmp,
            SortDirection::Desc => cmp.reverse(),
        }
    });
}

async fn get_issue(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Issue>>, ApiError> {
    match db_issue::get_issue(&deployment.db().pool, issue_id).await? {
        Some(issue) => Ok(ResponseJson(ApiResponse::success(issue))),
        None => Err(ApiError::BadRequest("Issue not found".to_string())),
    }
}

async fn create_issue(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateIssueRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<Issue>>>, ApiError> {
    let pool: &SqlitePool = &deployment.db().pool;

    // Ensure the project has default statuses so issue creation never hits empty state.
    db_status::ensure_default_statuses(pool, request.project_id).await?;

    // If the request references a status that doesn't exist yet (e.g. a fresh UUID from the
    // board UI), insert a placeholder status row so the FK constraint isn't violated.
    let statuses = db_status::list_project_statuses(pool, request.project_id).await?;
    if !statuses.iter().any(|s| s.id == request.status_id) {
        sqlx::query(
            "INSERT INTO project_statuses (id, project_id, name, color, sort_order, hidden, created_at, updated_at) \
             VALUES (?, ?, 'todo', '#3b82f6', 0, 0, datetime('now'), datetime('now'))",
        )
        .bind(request.status_id)
        .bind(request.project_id)
        .execute(pool)
        .await?;
    }

    let issue = db_issue::create_issue(pool, request.project_id, &request).await?;
    Ok(ResponseJson(ApiResponse::success(MutationResponse {
        data: issue,
        txid: 0,
    })))
}

async fn update_issue(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
    Json(request): Json<UpdateIssueRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<Issue>>>, ApiError> {
    match db_issue::update_issue(&deployment.db().pool, issue_id, &request).await? {
        Some(issue) => Ok(ResponseJson(ApiResponse::success(MutationResponse {
            data: issue,
            txid: 0,
        }))),
        None => Err(ApiError::BadRequest("Issue not found".to_string())),
    }
}

async fn delete_issue(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    db_issue::delete_issue(&deployment.db().pool, issue_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}
