//! Server-signed epoch bump tokens for cryptographic-erasure purge.
//!
//! When a privileged user successfully runs `purge_before` against a
//! channel, the server atomically increments the channel's epoch and
//! mints a short-lived RS256 JWT carrying:
//!
//!   * `channel_id`        — channel the bump applies to
//!   * `epoch`             — the server-issued new epoch (monotonic)
//!   * `before_message_id` — ULID watermark; everything <= this is erased
//!   * `purged_by`         — user id that triggered the purge
//!   * `deleted_count`     — number of messages the server actually removed
//!   * standard `aud` / `iat` / `exp`
//!
//! Clients in the channel verify this token before adopting the new
//! epoch. This is what stops a malicious group member from spoofing an
//! epoch bump and forcing peers to wipe their local message caches —
//! without verification, anyone in the channel could send a fake
//! `_e2e_epoch_bump` JSON and get every recipient to drop their cache.
//!
//! Reuses the cosmetics/external-auth RS256 keypair: the same private
//! key signs all Company-issued JWTs; `aud` is the only scoping
//! mechanism. The public key path is `external_auth.public_key_path`.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use once_cell::sync::OnceCell;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum EpochBumpError {
    KeyLoad(String),
    Sign(String),
}

impl std::fmt::Display for EpochBumpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpochBumpError::KeyLoad(s) => write!(f, "key load: {s}"),
            EpochBumpError::Sign(s) => write!(f, "sign: {s}"),
        }
    }
}

impl std::error::Error for EpochBumpError {}

static ENCODING_KEY: OnceCell<EncodingKey> = OnceCell::new();

fn encoding_key(private_key_path: &str) -> Result<&'static EncodingKey, EpochBumpError> {
    if let Some(k) = ENCODING_KEY.get() {
        return Ok(k);
    }
    let pem = std::fs::read_to_string(private_key_path)
        .map_err(|e| EpochBumpError::KeyLoad(format!("read {private_key_path}: {e}")))?;
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| EpochBumpError::KeyLoad(e.to_string()))?;
    let _ = ENCODING_KEY.set(key);
    Ok(ENCODING_KEY.get().expect("just set"))
}

#[derive(Serialize)]
struct EpochBumpClaims<'a> {
    aud: &'static str,
    iat: u64,
    exp: u64,
    channel_id: &'a str,
    epoch: u64,
    before_message_id: &'a str,
    purged_by: &'a str,
    deleted_count: u64,
}

pub const EPOCH_BUMP_AUDIENCE: &str = "company-epoch-bump";

/// TTL for an epoch bump token. 10 minutes — wide enough to survive
/// retries and clock skew across peers, tight enough to limit replay
/// of a captured token from a later session.
const TOKEN_TTL_SECS: u64 = 600;

pub fn mint_epoch_bump_token(
    private_key_path: &str,
    channel_id: &str,
    epoch: u64,
    before_message_id: &str,
    purged_by: &str,
    deleted_count: u64,
) -> Result<String, EpochBumpError> {
    let key = encoding_key(private_key_path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let claims = EpochBumpClaims {
        aud: EPOCH_BUMP_AUDIENCE,
        iat: now,
        exp: now + TOKEN_TTL_SECS,
        channel_id,
        epoch,
        before_message_id,
        purged_by,
        deleted_count,
    };
    encode(&Header::new(Algorithm::RS256), &claims, key)
        .map_err(|e| EpochBumpError::Sign(e.to_string()))
}
