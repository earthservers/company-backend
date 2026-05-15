//! Proxy helpers between Company API and the EarthCosmetics backend.
//!
//! Auth model: Company holds the RS256 private key; cosmetics backend has
//! the matching public key. Every proxied request mints a fresh short-lived
//! JWT with the appropriate audience claim — moderator tokens are scoped to
//! `aud=decoration`, creator submissions to `aud=cosmetics-submission`. The
//! mod-side player flows (Mojang HS256) are untouched.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use once_cell::sync::OnceCell;
use reqwest::{Client, StatusCode};
use rocket::serde::json::Value as JsonValue;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// Errors that can occur while proxying to the cosmetics backend.
#[derive(Debug)]
pub enum CosmeticsProxyError {
    /// Cosmetics backend isn't configured (missing url or key); operator must
    /// set `cosmetics.backend_url` + `cosmetics.private_key_path` in config.
    NotConfigured,
    /// Couldn't read or parse the private key file.
    KeyLoadError(String),
    /// JWT signing failed (algorithm mismatch, encoding error, etc.).
    SignError(String),
    /// HTTP request to cosmetics backend failed.
    HttpError(String),
    /// Cosmetics backend returned a non-2xx response. Carries the status + body.
    BackendError(StatusCode, JsonValue),
}

impl CosmeticsProxyError {
    pub fn user_message(&self) -> String {
        match self {
            Self::NotConfigured => "Cosmetics moderation is not configured on this server.".to_string(),
            Self::KeyLoadError(_) | Self::SignError(_) => "Cosmetics auth signing failed.".to_string(),
            Self::HttpError(e) => format!("Cosmetics backend unreachable: {e}"),
            Self::BackendError(s, _) => format!("Cosmetics backend returned {s}"),
        }
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    sub: &'a str,
    aud: &'a str,
    exp: u64,
    iat: u64,
}

static ENCODING_KEY: OnceCell<EncodingKey> = OnceCell::new();
static HTTP_CLIENT: OnceCell<Client> = OnceCell::new();

fn encoding_key(private_key_path: &str) -> Result<&'static EncodingKey, CosmeticsProxyError> {
    if let Some(k) = ENCODING_KEY.get() { return Ok(k); }
    let pem = std::fs::read_to_string(private_key_path)
        .map_err(|e| CosmeticsProxyError::KeyLoadError(format!("read {private_key_path}: {e}")))?;
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| CosmeticsProxyError::KeyLoadError(format!("parse PEM: {e}")))?;
    let _ = ENCODING_KEY.set(key);
    Ok(ENCODING_KEY.get().expect("just set"))
}

fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("build reqwest client")
    })
}

/// Mint a short-lived RS256 JWT for the given subject + audience.
pub fn mint_jwt(
    user_id: &str,
    audience: &str,
    ttl_secs: u64,
    private_key_path: &str,
) -> Result<String, CosmeticsProxyError> {
    let key = encoding_key(private_key_path)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let claims = Claims {
        sub: user_id,
        aud: audience,
        exp: now + ttl_secs,
        iat: now,
    };
    let header = Header::new(Algorithm::RS256);
    encode(&header, &claims, key).map_err(|e| CosmeticsProxyError::SignError(e.to_string()))
}

/// Claims for the Minecraft account-link JWT. Distinct shape from {@link Claims}
/// because the link flow needs two extra fields the cosmetics backend uses
/// to bind the token to a specific Mojang UUID and to prevent replay.
#[derive(Serialize)]
struct LinkClaims<'a> {
    sub: &'a str,
    aud: &'a str,
    exp: u64,
    iat: u64,
    /// Mojang UUID this token consents to link. The cosmetics backend
    /// rejects the request unless its caller's Mojang JWT carries the same
    /// UUID — so a token minted for player A cannot be redeemed against
    /// player B's session.
    mojang_uuid_intent: &'a str,
    /// Cryptographically random per-mint identifier. The cosmetics backend
    /// dedup-tracks consumed nonces for >= the JWT's exp window, so a
    /// captured token cannot be re-submitted.
    nonce: &'a str,
}

/// Mint the JWT used by the EarthCosmetics account-link flow.
///
/// `mojang_uuid_intent` is bound into the JWT so the token cannot be
/// silently re-targeted to a different Mojang account (the cosmetics
/// backend asserts it matches the caller's Mojang-JWT sub).
///
/// `nonce` MUST be unique per mint and unpredictable — caller is
/// responsible for generating it (e.g. `nanoid::nanoid!()` or `Ulid::new()`).
/// Reusing a nonce will be rejected as replay by the cosmetics backend.
pub fn mint_link_jwt(
    user_id: &str,
    audience: &str,
    ttl_secs: u64,
    private_key_path: &str,
    mojang_uuid_intent: &str,
    nonce: &str,
) -> Result<String, CosmeticsProxyError> {
    let key = encoding_key(private_key_path)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let claims = LinkClaims {
        sub: user_id,
        aud: audience,
        exp: now + ttl_secs,
        iat: now,
        mojang_uuid_intent,
        nonce,
    };
    let header = Header::new(Algorithm::RS256);
    encode(&header, &claims, key).map_err(|e| CosmeticsProxyError::SignError(e.to_string()))
}

/// Configuration view passed into each proxy call. Mirrors the relevant
/// fields of `config::Cosmetics` without depending on it directly so this
/// module stays unit-testable.
pub struct ProxyConfig<'a> {
    pub backend_url: &'a str,
    pub private_key_path: &'a str,
    pub audience: &'a str,
    pub ttl_secs: u64,
}

impl<'a> ProxyConfig<'a> {
    pub fn validated(&self) -> Result<(), CosmeticsProxyError> {
        if self.backend_url.is_empty() || self.private_key_path.is_empty() {
            return Err(CosmeticsProxyError::NotConfigured);
        }
        Ok(())
    }
}

async fn handle_response(resp: reqwest::Response) -> Result<JsonValue, CosmeticsProxyError> {
    let status = resp.status();
    let body: JsonValue = resp.json().await
        .unwrap_or_else(|_| serde_json::json!({ "error": "non_json_body" }));
    if !status.is_success() {
        return Err(CosmeticsProxyError::BackendError(status, body));
    }
    Ok(body)
}

/// GET helper.
pub async fn proxy_get(
    cfg: &ProxyConfig<'_>,
    user_id: &str,
    path: &str,
) -> Result<JsonValue, CosmeticsProxyError> {
    cfg.validated()?;
    let jwt = mint_jwt(user_id, cfg.audience, cfg.ttl_secs, cfg.private_key_path)?;
    let url = format!("{}{}", cfg.backend_url.trim_end_matches('/'), path);
    let resp = http_client()
        .get(&url)
        .bearer_auth(&jwt)
        .send()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(e.to_string()))?;
    handle_response(resp).await
}

/// POST helper.
pub async fn proxy_post(
    cfg: &ProxyConfig<'_>,
    user_id: &str,
    path: &str,
    body: &JsonValue,
) -> Result<JsonValue, CosmeticsProxyError> {
    cfg.validated()?;
    let jwt = mint_jwt(user_id, cfg.audience, cfg.ttl_secs, cfg.private_key_path)?;
    let url = format!("{}{}", cfg.backend_url.trim_end_matches('/'), path);
    let resp = http_client()
        .post(&url)
        .bearer_auth(&jwt)
        .json(body)
        .send()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(e.to_string()))?;
    handle_response(resp).await
}

/// PATCH helper.
pub async fn proxy_patch(
    cfg: &ProxyConfig<'_>,
    user_id: &str,
    path: &str,
    body: &JsonValue,
) -> Result<JsonValue, CosmeticsProxyError> {
    cfg.validated()?;
    let jwt = mint_jwt(user_id, cfg.audience, cfg.ttl_secs, cfg.private_key_path)?;
    let url = format!("{}{}", cfg.backend_url.trim_end_matches('/'), path);
    let resp = http_client()
        .patch(&url)
        .bearer_auth(&jwt)
        .json(body)
        .send()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(e.to_string()))?;
    handle_response(resp).await
}

/// DELETE helper. No body payload — DELETE is bodyless by convention here
/// (cosmetics backend's DELETE routes use path params for the target).
pub async fn proxy_delete(
    cfg: &ProxyConfig<'_>,
    user_id: &str,
    path: &str,
) -> Result<JsonValue, CosmeticsProxyError> {
    cfg.validated()?;
    let jwt = mint_jwt(user_id, cfg.audience, cfg.ttl_secs, cfg.private_key_path)?;
    let url = format!("{}{}", cfg.backend_url.trim_end_matches('/'), path);
    let resp = http_client()
        .delete(&url)
        .bearer_auth(&jwt)
        .send()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(e.to_string()))?;
    handle_response(resp).await
}
