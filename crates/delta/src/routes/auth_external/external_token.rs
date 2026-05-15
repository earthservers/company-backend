//! Exchange a Company session for an audience-scoped JWT.
//!
//! Two callers:
//!   1. First call after Company login: the SSO widget POSTs with
//!      `x-session-token: <token>` header. We mint the JWT and set
//!      `company_session=<token>; Domain=.earthservers.net` so subsequent
//!      requests on any earthservers.net subdomain can auth via cookie.
//!   2. Silent refresh from a different subdomain: browser sends the parent-
//!      domain cookie automatically. We mint a fresh JWT, no cookie rewrite.

use company_config::config;
use rocket::http::{Cookie, CookieJar, SameSite, Status};
use rocket::serde::json::Json;
use rocket::time::Duration;
use serde::{Deserialize, Serialize};

use crate::util::cosmetics_proxy::mint_jwt;
use crate::util::external_auth::CompanySession;

#[derive(Debug, Deserialize)]
pub struct ExternalTokenRequest {
    /// Audience claim for the returned JWT. Must be in the configured
    /// `external_auth.allowed_audiences` list, e.g. `"earthsocial"`.
    pub audience: String,
}

#[derive(Debug, Serialize)]
pub struct ExternalTokenResponse {
    pub jwt: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    #[serde(rename = "companyUserId")]
    pub company_user_id: String,
}

#[derive(Debug, Serialize)]
pub struct ExternalTokenError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Exchange a Company session for an audience-scoped JWT.
///
/// Takes the current Company session (via `x-session-token` header or the
/// `company_session` parent-domain cookie) and returns an RS256 JWT scoped to
/// the requested audience. Sets the parent-domain SSO cookie as a side effect.
#[post("/external-token", data = "<body>")]
pub async fn external_token(
    session: CompanySession,
    cookies: &CookieJar<'_>,
    body: Json<ExternalTokenRequest>,
) -> Result<Json<ExternalTokenResponse>, (Status, Json<ExternalTokenError>)> {
    let config = config().await;
    let ext = &config.external_auth;

    if !ext.allowed_audiences.iter().any(|a| a == &body.audience) {
        return Err((
            Status::Forbidden,
            Json(ExternalTokenError {
                error: "audience_not_allowed".to_owned(),
                detail: Some(format!("requested audience '{}' not configured", body.audience)),
            }),
        ));
    }

    let jwt = mint_jwt(
        &session.user_id,
        &body.audience,
        ext.user_token_ttl_secs,
        &ext.private_key_path,
    )
    .map_err(|e| {
        log::warn!("external-token mint failed: {:?}", e);
        (
            Status::InternalServerError,
            Json(ExternalTokenError {
                error: "mint_failed".to_owned(),
                detail: Some(e.user_message()),
            }),
        )
    })?;

    // Refresh the parent-domain cookie on every call. Operators can disable
    // the cookie write by setting `external_auth.session_cookie_domain = ""`.
    if !ext.session_cookie_domain.is_empty() {
        let cookie = Cookie::build(("company_session", session.session_token.clone()))
            .domain(ext.session_cookie_domain.clone())
            .path("/")
            .secure(true)
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(Duration::days(30))
            .build();
        cookies.add(cookie);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let exp = now + ext.user_token_ttl_secs;
    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(exp as i64, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default();

    Ok(Json(ExternalTokenResponse {
        jwt,
        expires_at,
        company_user_id: session.user_id,
    }))
}
