//! Per-connection device-bound RS256 JWT attestation for E2E pairing.
//!
//! When a user starts an E2E pairing (DM, group join, device pair), the
//! client first asks the server to attest its long-lived ECDH public
//! key. The server returns a short-lived RS256 JWT whose claims bind:
//!
//!   * `sub` — the authenticated Company user id
//!   * `dh_public_key_pem` — the exact PEM-encoded SPKI the client
//!     claims to own. Verbatim so receivers can compare byte-for-byte
//!     against the PEM in the pairing token metadata.
//!   * standard `aud` / `iat` / `exp`
//!
//! The receiving peer (the other party in the pairing handshake)
//! verifies the JWT against the server's RS256 public key and checks
//! both that `sub` matches the expected user-id of the peer AND that
//! `dh_public_key_pem` matches the PEM in the pairing claim. Together
//! these prove "this Company user-id owns this ECDH key, attested by
//! the server, within the last few minutes" — without trusting the
//! peer or the pairing-token relay.
//!
//! Reuses the cosmetics/external-auth RS256 keypair (same private key
//! that signs SSO and epoch-bump JWTs); audience is the only scoping
//! mechanism. The public key path is `external_auth.public_key_path`,
//! the same one already exposed to clients via the `/` config endpoint
//! under `features.epoch_signing.public_key_pem`.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use once_cell::sync::OnceCell;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum PairingAttestationError {
    KeyLoad(String),
    Sign(String),
}

impl std::fmt::Display for PairingAttestationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PairingAttestationError::KeyLoad(s) => write!(f, "key load: {s}"),
            PairingAttestationError::Sign(s) => write!(f, "sign: {s}"),
        }
    }
}

impl std::error::Error for PairingAttestationError {}

static ENCODING_KEY: OnceCell<EncodingKey> = OnceCell::new();

fn encoding_key(
    private_key_path: &str,
) -> Result<&'static EncodingKey, PairingAttestationError> {
    if let Some(k) = ENCODING_KEY.get() {
        return Ok(k);
    }
    let pem = std::fs::read_to_string(private_key_path).map_err(|e| {
        PairingAttestationError::KeyLoad(format!("read {private_key_path}: {e}"))
    })?;
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| PairingAttestationError::KeyLoad(e.to_string()))?;
    let _ = ENCODING_KEY.set(key);
    Ok(ENCODING_KEY.get().expect("just set"))
}

#[derive(Serialize)]
struct PairingAttestationClaims<'a> {
    aud: &'static str,
    iat: u64,
    exp: u64,
    sub: &'a str,
    dh_public_key_pem: &'a str,
}

pub const PAIRING_ATTESTATION_AUDIENCE: &str = "company-pairing-attestation";

/// TTL for an attestation. 10 minutes — long enough that a single
/// attestation covers multiple consecutive pairings (e.g. fanning out
/// to several DMs), short enough that a captured token can't be
/// replayed against a future pairing.
const TOKEN_TTL_SECS: u64 = 600;

pub fn mint_pairing_attestation_token(
    private_key_path: &str,
    user_id: &str,
    dh_public_key_pem: &str,
) -> Result<String, PairingAttestationError> {
    let key = encoding_key(private_key_path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let claims = PairingAttestationClaims {
        aud: PAIRING_ATTESTATION_AUDIENCE,
        iat: now,
        exp: now + TOKEN_TTL_SECS,
        sub: user_id,
        dh_public_key_pem,
    };
    encode(&Header::new(Algorithm::RS256), &claims, key)
        .map_err(|e| PairingAttestationError::Sign(e.to_string()))
}
