use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::middleware::RequestContext;

#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityResponse {
    pub user_id: Uuid,
    pub username: Option<String>,
    pub email: String,
}

pub(super) async fn get_identity(
    Extension(ctx): Extension<RequestContext>,
) -> Json<IdentityResponse> {
    Json(IdentityResponse {
        user_id: ctx.user_id,
        username: ctx.username,
        email: ctx.email,
    })
}
