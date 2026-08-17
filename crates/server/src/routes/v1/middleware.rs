use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{V1State, jwt::JwtError};

/// Authenticated caller context extracted from the Bearer access token.
#[derive(Clone)]
pub struct RequestContext {
    pub user_id: Uuid,
    pub username: Option<String>,
    pub email: String,
    pub session_id: Uuid,
    #[allow(dead_code)]
    pub access_token_expires_at: DateTime<Utc>,
}

/// Auth middleware: require a valid `Authorization: Bearer <access>` token,
/// verify the underlying session and user, and expose a `RequestContext`
/// extension for downstream handlers.
pub async fn require_session(
    State(state): State<V1State>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let Some(bearer) = bearer_token(req.headers()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let ctx = match request_context_from_access_token(&state, &bearer).await {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };

    req.extensions_mut().insert(ctx);
    next.run(req).await
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_string())
        .filter(|t| !t.is_empty())
}

async fn request_context_from_access_token(
    state: &V1State,
    access_token: &str,
) -> Result<RequestContext, Response> {
    let details = match state.jwt.decode_access_token(access_token) {
        Ok(d) => d,
        Err(JwtError::TokenExpired) => return Err(StatusCode::UNAUTHORIZED.into_response()),
        Err(_) => return Err(StatusCode::UNAUTHORIZED.into_response()),
    };

    let session = match db::models::auth_session::find_by_id(&state.pool, details.session_id).await
    {
        Ok(Some(s)) => s,
        Ok(None) => {
            tracing::warn!(session_id = %details.session_id, "session not found");
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load auth session");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    if session.user_id != details.user_id {
        tracing::warn!(
            token_user_id = %details.user_id,
            session_user_id = %session.user_id,
            "access token user does not match session user"
        );
        return Err(StatusCode::UNAUTHORIZED.into_response());
    }

    // Expiry check against the row's stored `expires_at` (refresh-token TTL).
    if let Ok(expires_at) = DateTime::parse_from_rfc3339(&session.expires_at) {
        if expires_at.with_timezone(&Utc) < Utc::now() {
            let _ = db::models::auth_session::invalidate(&state.pool, session.id).await;
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
    }

    let user = match db::models::user::find_user_by_id(&state.pool, details.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            tracing::warn!(user_id = %details.user_id, "user missing");
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load user");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    let _ = db::models::auth_session::touch(&state.pool, session.id).await;

    Ok(RequestContext {
        user_id: user.id,
        username: user.username,
        email: user.email,
        session_id: session.id,
        access_token_expires_at: details.expires_at,
    })
}
