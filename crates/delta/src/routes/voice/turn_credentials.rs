use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use company_config::config;
use company_database::User;
use company_result::{create_error, Result};
use hmac::{Hmac, Mac};
use rocket::serde::json::Json;
use schemars::JsonSchema;
use serde::Serialize;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Time-limited TURN credentials minted via coturn's REST API auth
/// scheme. Username is `<expiry_unix>:<user_id>`, password is the
/// base64-encoded HMAC-SHA1 of the username keyed by coturn's
/// `static-auth-secret`. coturn verifies the HMAC on its own.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TurnCredentials {
    /// Username (encodes the expiry).
    pub username: String,
    /// HMAC-SHA1 of the username, base64-encoded.
    pub password: String,
    /// TURN server URLs, e.g. `turns:turn.example.net:5349`.
    pub urls: Vec<String>,
    /// coturn realm value (may be empty).
    pub realm: String,
    /// Seconds from now until the credentials expire.
    pub ttl_seconds: u64,
}

/// # Get TURN credentials
///
/// Returns short-lived TURN credentials for non-LiveKit WebRTC
/// sessions (P2P voice calls, beacon file transfer). Authenticated
/// users only — the user id is embedded in the username so coturn
/// logs can be traced back to the originating account.
///
/// Returns 503 when `[turn].static_auth_secret` is unset on the
/// server (TURN minting is disabled).
#[openapi(tag = "Voice")]
#[get("/turn-credentials")]
pub async fn turn_credentials(user: User) -> Result<Json<TurnCredentials>> {
    let cfg = config().await;
    let turn = cfg.turn;

    if turn.static_auth_secret.is_empty() {
        return Err(create_error!(InternalError));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ttl = turn.ttl_secs;
    let expiry = now + ttl;

    // Format per coturn REST API spec:
    //   username = "<unix_expiry>:<arbitrary_id>"
    //   password = base64(HMAC-SHA1(username, static_auth_secret))
    let username = format!("{}:{}", expiry, user.id);
    let mut mac = HmacSha1::new_from_slice(turn.static_auth_secret.as_bytes())
        .map_err(|_| create_error!(InternalError))?;
    mac.update(username.as_bytes());
    let password = B64.encode(mac.finalize().into_bytes());

    Ok(Json(TurnCredentials {
        username,
        password,
        urls: turn.urls,
        realm: turn.realm,
        ttl_seconds: ttl,
    }))
}
