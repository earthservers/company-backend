//! External-service auth primitives.
//!
//! Company issues two flavours of RS256 JWT to non-Company callers:
//!   * **User tokens** — aud=`<consumer>` (e.g. `earthsocial`), sub=user ULID.
//!     Minted via `cosmetics_proxy::mint_jwt` and consumed by the cosmetics
//!     backend, EarthSocial, etc.
//!   * **Service tokens** — aud=`company-internal`, sub=`<client_id>`, plus a
//!     `scope` claim. Used for backend-to-backend reads (e.g. EarthSocial
//!     hydrating its user mirror by calling `GET /users/:id`). Verified by
//!     Company itself via the `ServiceIdentity` guard below.
//!
//! Same key signs both flavours; audience is the only scoping mechanism.

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::OnceCell;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SERVICE_AUDIENCE: &str = "company-internal";

/// Errors raised when minting or verifying a service token.
#[derive(Debug)]
pub enum ExternalAuthError {
    /// Key file path is unset, unreadable, or unparseable.
    KeyLoadError(String),
    /// JWT signing failed.
    SignError(String),
    /// JWT signature, audience, or expiry check failed.
    VerifyError(String),
    /// Scope check failed.
    InsufficientScope { required: String, actual: String },
    /// `aud` claim didn't match `company-internal`.
    WrongAudience(String),
}

impl ExternalAuthError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::KeyLoadError(_) => "External auth signing key not configured.",
            Self::SignError(_) => "External auth signing failed.",
            Self::VerifyError(_) => "Invalid or expired service token.",
            Self::InsufficientScope { .. } => "Service token missing required scope.",
            Self::WrongAudience(_) => "Service token has wrong audience.",
        }
    }
}

#[derive(Serialize)]
struct ServiceClaimsOut<'a> {
    sub: &'a str,
    aud: &'a str,
    scope: &'a str,
    iss: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceClaims {
    pub sub: String,
    pub aud: String,
    pub scope: String,
    pub exp: u64,
    pub iat: u64,
}

static ENCODING_KEY: OnceCell<EncodingKey> = OnceCell::new();
static DECODING_KEY: OnceCell<DecodingKey> = OnceCell::new();

fn encoding_key(private_key_path: &str) -> Result<&'static EncodingKey, ExternalAuthError> {
    if let Some(k) = ENCODING_KEY.get() {
        return Ok(k);
    }
    let pem = std::fs::read_to_string(private_key_path)
        .map_err(|e| ExternalAuthError::KeyLoadError(format!("read {private_key_path}: {e}")))?;
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| ExternalAuthError::KeyLoadError(format!("parse private PEM: {e}")))?;
    let _ = ENCODING_KEY.set(key);
    Ok(ENCODING_KEY.get().expect("just set"))
}

fn decoding_key(public_key_path: &str) -> Result<&'static DecodingKey, ExternalAuthError> {
    if let Some(k) = DECODING_KEY.get() {
        return Ok(k);
    }
    let pem = std::fs::read_to_string(public_key_path)
        .map_err(|e| ExternalAuthError::KeyLoadError(format!("read {public_key_path}: {e}")))?;
    let key = DecodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| ExternalAuthError::KeyLoadError(format!("parse public PEM: {e}")))?;
    let _ = DECODING_KEY.set(key);
    Ok(DECODING_KEY.get().expect("just set"))
}

/// Sign a service-audience JWT.
pub fn mint_service_jwt(
    client_id: &str,
    scope: &str,
    ttl_secs: u64,
    private_key_path: &str,
) -> Result<(String, u64), ExternalAuthError> {
    let key = encoding_key(private_key_path)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let exp = now + ttl_secs;
    let claims = ServiceClaimsOut {
        sub: client_id,
        aud: SERVICE_AUDIENCE,
        scope,
        iss: "company",
        iat: now,
        exp,
    };
    let token = encode(&Header::new(Algorithm::RS256), &claims, key)
        .map_err(|e| ExternalAuthError::SignError(e.to_string()))?;
    Ok((token, exp))
}

/// Verify a service-audience JWT. Returns the parsed claims on success.
/// Does not enforce a specific scope — that's the caller's responsibility,
/// because different routes need different scopes.
pub fn verify_service_jwt(
    token: &str,
    public_key_path: &str,
) -> Result<ServiceClaims, ExternalAuthError> {
    let key = decoding_key(public_key_path)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[SERVICE_AUDIENCE]);
    validation.validate_exp = true;
    decode::<ServiceClaims>(token, key, &validation)
        .map(|d| d.claims)
        .map_err(|e| ExternalAuthError::VerifyError(e.to_string()))
}

/// Constant-time `&[u8]` equality. Used to compare client secrets so
/// timing attacks can't enumerate them.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (ai, bi) in a.iter().zip(b.iter()) {
        diff |= ai ^ bi;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// Rocket request guards
// ---------------------------------------------------------------------------

/// Verified service identity. Routes that accept service tokens take this
/// guard and check `scope` themselves.
#[derive(Debug, Clone)]
pub struct ServiceIdentity {
    pub client_id: String,
    pub scope: String,
}

impl ServiceIdentity {
    /// Return true if the token's space-separated scope list contains `wanted`.
    pub fn has_scope(&self, wanted: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == wanted)
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ServiceIdentity {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(auth_header) = request.headers().get_one("Authorization") else {
            return Outcome::Forward(Status::Unauthorized);
        };
        let Some(token) = auth_header.strip_prefix("Bearer ") else {
            return Outcome::Forward(Status::Unauthorized);
        };

        let config = company_config::config().await;
        let public_key_path = &config.external_auth.public_key_path;

        match verify_service_jwt(token.trim(), public_key_path) {
            Ok(claims) => Outcome::Success(ServiceIdentity {
                client_id: claims.sub,
                scope: claims.scope,
            }),
            Err(_) => Outcome::Forward(Status::Unauthorized),
        }
    }
}

/// Resolves the current Company session via either:
///   * `x-session-token` request header, or
///   * `company_session` cookie (parent-domain SSO cookie).
///
/// Returns both the verified `user_id` and the raw token so callers can
/// (re-)set the cookie on response. Used by `POST /auth/external-token`.
#[derive(Debug, Clone)]
pub struct CompanySession {
    pub user_id: String,
    pub session_token: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for CompanySession {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let token = request
            .headers()
            .get_one("x-session-token")
            .map(|s| s.to_string())
            .or_else(|| {
                request
                    .cookies()
                    .get("company_session")
                    .map(|c| c.value().to_string())
            });

        let Some(token) = token else {
            return Outcome::Forward(Status::Unauthorized);
        };

        let db = match request.rocket().state::<company_database::Database>() {
            Some(db) => db,
            None => return Outcome::Forward(Status::InternalServerError),
        };

        match db.fetch_session_by_token(&token).await {
            Ok(session) => Outcome::Success(CompanySession {
                user_id: session.user_id,
                session_token: token,
            }),
            Err(_) => Outcome::Forward(Status::Unauthorized),
        }
    }
}
