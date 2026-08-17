use api_types::{
    AuthMethodsResponse, LocalLoginRequest, LocalLoginResponse, TokenRefreshRequest,
    TokenRefreshResponse,
};
use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{Duration as ChronoDuration, Utc};
use uuid::Uuid;

use super::{
    V1State,
    jwt::{JwtError, REFRESH_TOKEN_TTL_DAYS},
    middleware::RequestContext,
};

#[derive(Debug, thiserror::Error)]
pub(super) enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid token")]
    InvalidToken,
    #[error("session has been revoked")]
    SessionRevoked,
    #[error("refresh token reused - possible theft")]
    TokenReuseDetected,
    #[error(transparent)]
    Jwt(#[from] JwtError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
            AuthError::SessionRevoked => (StatusCode::UNAUTHORIZED, "session_revoked"),
            AuthError::TokenReuseDetected => (StatusCode::UNAUTHORIZED, "token_reuse_detected"),
            AuthError::Jwt(JwtError::TokenExpired) => (StatusCode::UNAUTHORIZED, "expired_token"),
            AuthError::Jwt(_) => (StatusCode::UNAUTHORIZED, "invalid_token"),
            AuthError::Database(err) => {
                tracing::error!(error = %err, "database error during auth");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        (status, Json(serde_json::json!({ "error": code }))).into_response()
    }
}

pub(super) async fn auth_methods(State(_state): State<V1State>) -> Json<AuthMethodsResponse> {
    // Self-hosted backend: always offer local auth, no OAuth providers.
    Json(AuthMethodsResponse {
        local_auth_enabled: true,
        oauth_providers: Vec::new(),
    })
}

pub(super) async fn local_login(
    State(state): State<V1State>,
    Json(payload): Json<LocalLoginRequest>,
) -> Result<Json<LocalLoginResponse>, AuthError> {
    if !super::local_credential(&payload.email, &payload.password) {
        return Err(AuthError::InvalidCredentials);
    }

    let username = payload.email.split('@').next().map(|s| s.to_string());
    let user =
        db::models::user::upsert_user(&state.pool, &payload.email, username.as_deref()).await?;
    let _ = db::models::organization::ensure_personal_org_and_admin_membership(
        &state.pool,
        user.id,
        user.username.as_deref(),
    )
    .await?;

    let session_id = Uuid::new_v4();
    let tokens = state.jwt.generate_tokens(user.id, session_id, "local")?;

    db::models::auth_session::create(
        &state.pool,
        session_id,
        user.id,
        &tokens.refresh_token_id.to_string(),
        ChronoDuration::days(REFRESH_TOKEN_TTL_DAYS),
    )
    .await?;

    Ok(Json(LocalLoginResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    }))
}

pub(super) async fn refresh_token(
    State(state): State<V1State>,
    Json(payload): Json<TokenRefreshRequest>,
) -> Result<Json<TokenRefreshResponse>, AuthError> {
    let details = match state.jwt.decode_refresh_token(&payload.refresh_token) {
        Ok(d) => d,
        Err(JwtError::TokenExpired) => {
            return Err(AuthError::InvalidToken);
        }
        Err(_) => return Err(AuthError::InvalidToken),
    };

    let session =
        match db::models::auth_session::find_by_id(&state.pool, details.session_id).await? {
            Some(s) => s,
            None => return Err(AuthError::SessionRevoked),
        };

    if session.user_id != details.user_id {
        return Err(AuthError::InvalidToken);
    }

    // Rotation: the presented refresh token must be the current one for this
    // session. Reuse of an older token is treated as theft and revokes the
    // session.
    let presented = details.refresh_token_id.to_string();
    match session.refresh_token_id.as_deref() {
        Some(stored) if stored == presented => {}
        _ => {
            tracing::warn!(
                session_id = %session.id,
                "refresh token reuse detected; invalidating session"
            );
            let _ = db::models::auth_session::invalidate(&state.pool, session.id).await;
            return Err(AuthError::TokenReuseDetected);
        }
    }

    let now = Utc::now();
    let tokens = state.jwt.generate_tokens_for_refresh_token_id(
        session.user_id,
        session.id,
        &details.provider,
        Uuid::new_v4(),
        now,
    )?;
    db::models::auth_session::rotate_refresh_token(
        &state.pool,
        session.id,
        &tokens.refresh_token_id.to_string(),
    )
    .await?;

    Ok(Json(TokenRefreshResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    }))
}

pub(super) async fn logout(
    State(state): State<V1State>,
    Extension(ctx): Extension<RequestContext>,
) -> StatusCode {
    let _ = db::models::auth_session::invalidate(&state.pool, ctx.session_id).await;
    StatusCode::NO_CONTENT
}
