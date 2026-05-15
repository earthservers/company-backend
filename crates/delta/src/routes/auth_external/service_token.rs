//! Client-credentials grant for backend-to-backend service tokens.
//!
//! Used by EarthSocial backend (and future Earth Servers services) to read
//! `/users/:id` and similar service-token-gated routes without acting on
//! behalf of a logged-in user. Secrets live in
//! `config.external_auth.service_clients` and are compared in constant time.
//! Tokens are short-lived (1h default) so a leaked secret can only be abused
//! until the operator rotates and restarts.

use company_config::config;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};

use crate::util::external_auth::{
    constant_time_eq, mint_service_jwt, SERVICE_AUDIENCE,
};

#[derive(Debug, Deserialize)]
pub struct ServiceTokenRequest {
    pub client_id: String,
    pub client_secret: String,
    /// Must be `"company-internal"`. Other audiences are user-scoped and
    /// minted via `/auth/external-token` after a Company login.
    pub audience: String,
    /// Space-separated scope list, e.g. `"users:read"`.
    #[serde(default)]
    pub scope: String,
}

#[derive(Debug, Serialize)]
pub struct ServiceTokenResponse {
    pub jwt: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    pub scope: String,
}

#[derive(Debug, Serialize)]
pub struct ServiceTokenError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Issue a service token via client-credentials grant.
///
/// Returns an RS256 JWT with `aud=company-internal` for backend-to-backend reads.
#[post("/service-token", data = "<body>")]
pub async fn service_token(
    body: Json<ServiceTokenRequest>,
) -> Result<Json<ServiceTokenResponse>, (Status, Json<ServiceTokenError>)> {
    let body = body.into_inner();

    if body.audience != SERVICE_AUDIENCE {
        return Err((
            Status::Forbidden,
            Json(ServiceTokenError {
                error: "audience_not_allowed".to_owned(),
                detail: Some(format!(
                    "service tokens must request audience '{SERVICE_AUDIENCE}'"
                )),
            }),
        ));
    }

    let config = config().await;
    let ext = &config.external_auth;

    let expected = match ext.service_clients.get(&body.client_id) {
        Some(s) => s,
        None => {
            // Don't leak whether the client_id exists.
            return Err((
                Status::Unauthorized,
                Json(ServiceTokenError {
                    error: "invalid_client".to_owned(),
                    detail: None,
                }),
            ));
        }
    };

    if !constant_time_eq(expected.as_bytes(), body.client_secret.as_bytes()) {
        return Err((
            Status::Unauthorized,
            Json(ServiceTokenError {
                error: "invalid_client".to_owned(),
                detail: None,
            }),
        ));
    }

    let (jwt, exp) = mint_service_jwt(
        &body.client_id,
        &body.scope,
        ext.service_token_ttl_secs,
        &ext.private_key_path,
    )
    .map_err(|e| {
        log::warn!("service-token mint failed: {:?}", e);
        (
            Status::InternalServerError,
            Json(ServiceTokenError {
                error: "mint_failed".to_owned(),
                detail: Some(e.user_message().to_owned()),
            }),
        )
    })?;

    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(exp as i64, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default();

    Ok(Json(ServiceTokenResponse {
        jwt,
        expires_at,
        scope: body.scope,
    }))
}
