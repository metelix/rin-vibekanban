use std::sync::Arc;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// Access tokens are intentionally short-lived (2 minutes); clients refresh via
// the long-lived refresh token on `/v1/tokens/refresh`.
pub(super) const ACCESS_TOKEN_TTL_SECONDS: i64 = 120;
pub(super) const REFRESH_TOKEN_TTL_DAYS: i64 = 365;

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("invalid token")]
    InvalidToken,
    #[error("invalid jwt secret")]
    InvalidSecret,
    #[error("token expired")]
    TokenExpired,
    #[error("token type mismatch")]
    InvalidTokenType,
    #[error(transparent)]
    Jwt(#[from] jsonwebtoken::errors::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccessTokenClaims {
    pub sub: Uuid,
    pub session_id: Uuid,
    pub iat: i64,
    pub exp: i64,
    pub aud: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RefreshTokenClaims {
    pub sub: Uuid,
    pub session_id: Uuid,
    pub jti: Uuid,
    pub iat: i64,
    pub exp: i64,
    pub aud: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccessTokenDetails {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RefreshTokenDetails {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub refresh_token_id: Uuid,
    pub provider: String,
}

#[derive(Debug, Clone)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub refresh_token_id: Uuid,
}

/// HMAC-SHA256 JWT signer/verifier. PostgreSQL-independent — copied and
/// simplified from `crates/remote/src/auth/jwt.rs` for the self-hosted SQLite
/// backend. The secret is expected to be base64-encoded (numeric/alphanumeric
/// base64 per `jsonwebtoken::EncodingKey::from_base64_secret`).
#[derive(Clone)]
pub struct JwtService {
    secret: Arc<String>,
}

impl JwtService {
    pub fn new(secret: String) -> Self {
        Self {
            secret: Arc::new(secret),
        }
    }

    /// Issue a fresh short-lived access token and a fresh long-lived refresh
    /// token for the given (user, session) pair.
    pub fn generate_tokens(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        provider: &str,
    ) -> Result<Tokens, JwtError> {
        let now = Utc::now();
        let refresh_token_id = Uuid::new_v4();

        let access_token = self.encode_access_token(user_id, session_id, now)?;
        let refresh_token =
            self.encode_refresh_token(user_id, session_id, refresh_token_id, provider, now)?;

        Ok(Tokens {
            access_token,
            refresh_token,
            refresh_token_id,
        })
    }

    /// Re-issue an access token for a session whose refresh token was just
    /// rotated (keeps the same session id, fresh refresh token id).
    pub fn generate_tokens_for_refresh_token_id(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        provider: &str,
        refresh_token_id: Uuid,
        issued_at: DateTime<Utc>,
    ) -> Result<Tokens, JwtError> {
        let access_token = self.encode_access_token(user_id, session_id, issued_at)?;
        let refresh_token =
            self.encode_refresh_token(user_id, session_id, refresh_token_id, provider, issued_at)?;
        Ok(Tokens {
            access_token,
            refresh_token,
            refresh_token_id,
        })
    }

    fn encode_access_token(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<String, JwtError> {
        let access_exp = now + ChronoDuration::seconds(ACCESS_TOKEN_TTL_SECONDS);
        let claims = AccessTokenClaims {
            sub: user_id,
            session_id,
            iat: now.timestamp(),
            exp: access_exp.timestamp(),
            aud: "access".to_string(),
        };
        let key = EncodingKey::from_base64_secret(self.secret.as_str())?;
        Ok(encode(&Header::new(Algorithm::HS256), &claims, &key)?)
    }

    fn encode_refresh_token(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        refresh_token_id: Uuid,
        provider: &str,
        issued_at: DateTime<Utc>,
    ) -> Result<String, JwtError> {
        let refresh_exp = issued_at + ChronoDuration::days(REFRESH_TOKEN_TTL_DAYS);
        let claims = RefreshTokenClaims {
            sub: user_id,
            session_id,
            jti: refresh_token_id,
            iat: issued_at.timestamp(),
            exp: refresh_exp.timestamp(),
            aud: "refresh".to_string(),
            provider: Some(provider.to_string()),
        };
        let key = EncodingKey::from_base64_secret(self.secret.as_str())?;
        Ok(encode(&Header::new(Algorithm::HS256), &claims, &key)?)
    }

    pub fn decode_access_token(&self, token: &str) -> Result<AccessTokenDetails, JwtError> {
        if token.trim().is_empty() {
            return Err(JwtError::InvalidToken);
        }
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.validate_nbf = false;
        validation.set_audience(&["access"]);
        validation.required_spec_claims = std::collections::HashSet::from([
            "sub".to_string(),
            "exp".to_string(),
            "aud".to_string(),
        ]);

        let key = DecodingKey::from_base64_secret(self.secret.as_str())?;
        let data = decode::<AccessTokenClaims>(token, &key, &validation)?;
        let claims = data.claims;
        let expires_at = DateTime::from_timestamp(claims.exp, 0).ok_or(JwtError::InvalidToken)?;
        Ok(AccessTokenDetails {
            user_id: claims.sub,
            session_id: claims.session_id,
            expires_at,
        })
    }

    pub fn decode_refresh_token(&self, token: &str) -> Result<RefreshTokenDetails, JwtError> {
        if token.trim().is_empty() {
            return Err(JwtError::InvalidToken);
        }
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.validate_nbf = false;
        validation.set_audience(&["refresh"]);
        validation.required_spec_claims = std::collections::HashSet::from([
            "sub".to_string(),
            "exp".to_string(),
            "aud".to_string(),
            "jti".to_string(),
        ]);

        let key = DecodingKey::from_base64_secret(self.secret.as_str())?;
        let data = decode::<RefreshTokenClaims>(token, &key, &validation)?;
        let claims = data.claims;
        let provider = claims.provider.unwrap_or_else(|| "local".to_string());
        Ok(RefreshTokenDetails {
            user_id: claims.sub,
            session_id: claims.session_id,
            refresh_token_id: claims.jti,
            provider,
        })
    }
}
