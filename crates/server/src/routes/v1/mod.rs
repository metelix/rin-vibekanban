use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, post},
};
use base64::Engine as _;
use deployment::Deployment;
use sqlx::SqlitePool;
use utils::assets::asset_dir;

use crate::DeploymentImpl;

mod auth;
mod identity;
mod jwt;
mod middleware;
mod orgs;
mod projects;

pub use jwt::JwtService;

const DEFAULT_JWT_SECRET_DEV: &str =
    "cnNtLXZpa2VrYW5iYW4tZGV2LWp3dC1zZWNyZXQta2V5LXBhZGRpbmctdG8tNjQtYnl0ZXM";

/// Shared state for the `/v1/*` auth APIs: a handle to the local SQLite pool and
/// the JWT signer/verifier keyed off `VIBEKANBAN_JWT_SECRET`.
#[derive(Clone)]
pub struct V1State {
    pub pool: SqlitePool,
    pub jwt: JwtService,
}

/// Resolve the JWT signing secret. Priority: `VIBEKANBAN_JWT_SECRET` env var,
/// then a secret persisted in the asset dir (created on first boot), then a
/// hardcoded dev fallback. The returned value must be base64-encoded text
/// (it is fed to `EncodingKey::from_base64_secret`).
fn jwt_secret() -> String {
    if let Ok(s) = std::env::var("VIBEKANBAN_JWT_SECRET") {
        if !s.trim().is_empty() {
            tracing::info!("Using JWT secret from VIBEKANBAN_JWT_SECRET env var");
            return s;
        }
    }

    let path = asset_dir().join("v1-jwt-secret");
    if let Ok(s) = std::fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }

    let bytes: [u8; 32] = rand::random();
    let generated = base64::engine::general_purpose::STANDARD.encode(bytes);
    if std::fs::create_dir_all(asset_dir()).is_ok() {
        if std::fs::write(&path, &generated).is_ok() {
            tracing::info!("Generated and persisted a new JWT secret at {:?}", path);
            return generated;
        }
    }
    tracing::warn!("Using hardcoded dev JWT secret fallback");
    DEFAULT_JWT_SECRET_DEV.to_string()
}

fn local_credential(email: &str, password: &str) -> bool {
    // Self-host / dev mode: accept any non-empty credentials unless a fixed
    // env-var pair is configured.
    if email.trim().is_empty() || password.trim().is_empty() {
        return false;
    }
    let configured_email = std::env::var("VIBEKANBAN_LOCAL_EMAIL").ok();
    let configured_password = std::env::var("VIBEKANBAN_LOCAL_PASSWORD").ok();
    match (configured_email, configured_password) {
        (Some(e), Some(p)) if !e.trim().is_empty() && !p.trim().is_empty() => {
            email == e && password == p
        }
        _ => true,
    }
}

pub fn router(deployment: &DeploymentImpl) -> Router {
    let pool: SqlitePool = deployment.db().pool.clone();
    let jwt = JwtService::new(jwt_secret());
    let state = V1State { pool, jwt };

    // Public (unauthenticated) routes.
    let v1_public = Router::new()
        .route(
            "/auth/methods",
            get(auth::auth_methods).post(auth::auth_methods),
        )
        .route("/auth/local/login", post(auth::local_login))
        .route("/tokens/refresh", post(auth::refresh_token));

    // Protected routes, gated by the Bearer access-token middleware.
    let v1_protected = Router::new()
        .route("/identity", get(identity::get_identity))
        .route("/oauth/logout", post(auth::logout))
        .route("/organizations", get(orgs::list_organizations))
        .route("/organizations/{id}", get(orgs::get_organization))
        .route("/projects", get(projects::list_projects))
        .route("/projects", post(projects::create_project))
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::require_session,
        ));

    Router::new()
        .merge(v1_public)
        .merge(v1_protected)
        .with_state(state)
}
