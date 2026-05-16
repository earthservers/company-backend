//! `POST /pairing-attest`
//!
//! Mint a server-signed RS256 attestation binding the authenticated
//! user's id to their ECDH public key. Used by the E2E pairing flow:
//! each peer includes its attestation as claim metadata when claiming
//! the pairing token, and the other side verifies it before deriving
//! the shared session key.
//!
//! This prevents a man-in-the-middle who has somehow obtained a
//! pairing token (e.g. via a leaked deep link) from claiming it with
//! their own DH public key while spoofing the legitimate user's
//! identity — without a valid server attestation, the receiving peer
//! refuses to derive a session key for the wrong DH pubkey + user-id
//! combination.

use company_config::config;
use company_database::User;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};

use crate::util::pairing_attestation::mint_pairing_attestation_token;

#[derive(Deserialize)]
pub struct PairingAttestRequest {
    /// PEM-encoded SPKI of the client's ECDH P-256 public key. The
    /// server signs this verbatim into the JWT so receiving peers can
    /// byte-for-byte compare against the PEM in the pairing claim.
    pub dh_public_key_pem: String,
}

#[derive(Serialize)]
pub struct PairingAttestResponse {
    /// RS256 JWT (aud=`company-pairing-attestation`) the caller
    /// includes in their next pairing-token claim. 10-minute TTL.
    pub token: String,
}

#[derive(Serialize)]
pub struct PairingAttestError {
    pub error: String,
    pub detail: Option<String>,
}

/// Mint a pairing attestation for the authenticated user's DH key.
#[post("/pairing-attest", data = "<body>")]
pub async fn pairing_attest(
    user: User,
    body: Json<PairingAttestRequest>,
) -> Result<Json<PairingAttestResponse>, (Status, Json<PairingAttestError>)> {
    // Cheap sanity bounds: ECDH P-256 SPKI in PEM is ~150-200 bytes.
    // Reject anything outside a reasonable envelope so callers can't
    // smuggle large blobs into the signed claim.
    let pem = body.dh_public_key_pem.trim();
    if pem.len() < 100 || pem.len() > 1024 {
        return Err((
            Status::BadRequest,
            Json(PairingAttestError {
                error: "invalid_dh_public_key_pem".to_owned(),
                detail: Some(format!("length {} not in 100..1024", pem.len())),
            }),
        ));
    }
    if !pem.starts_with("-----BEGIN PUBLIC KEY-----") {
        return Err((
            Status::BadRequest,
            Json(PairingAttestError {
                error: "invalid_dh_public_key_pem".to_owned(),
                detail: Some("missing SPKI PEM armor".to_owned()),
            }),
        ));
    }

    let config = config().await;
    let token = mint_pairing_attestation_token(
        &config.external_auth.private_key_path,
        &user.id,
        pem,
    )
    .map_err(|e| {
        log::warn!("pairing-attest mint failed: {e}");
        (
            Status::InternalServerError,
            Json(PairingAttestError {
                error: "mint_failed".to_owned(),
                detail: None,
            }),
        )
    })?;

    Ok(Json(PairingAttestResponse { token }))
}
