//! Earth Nexus client — fetches subscription state for a Company user.
//!
//! Architecture: Nexus is the source of truth for all Stripe subscriptions
//! (and one-shot coin purchases). Company maintains a *cache* of each
//! user's tier in the `user.subscription` field so the synchronous-ish
//! entitlement-read sites scattered throughout the codebase
//! (`User::active_subscription_tier()`, voice channel gates, file upload
//! limits, etc.) don't need to become async network calls.
//!
//! Refresh policy (in priority order):
//!   1. Explicit `POST /users/me/subscription-refresh` after the user
//!      completes a Stripe Checkout — instant feedback in the UI.
//!   2. Per-request staleness check: if `user.subscription` wasn't refreshed
//!      within `config.nexus.refresh_interval_secs`, fire a background
//!      refresh. The current request still reads the stale cache; the
//!      *next* request sees the fresh value. Cheap, doesn't add latency.
//!   3. Login: TODO — wire into authifier session creation.
//!
//! Auth: Company mints a fresh RS256 JWT with `aud=earth-nexus, sub=<user>`
//! using the same private key it uses for cosmetics moderation. No
//! API-key-per-service — Nexus identifies the *user* the call is made on
//! behalf of via the JWT's `sub` claim.

use std::time::{SystemTime, UNIX_EPOCH};

use company_config::config;
use company_database::Database;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use once_cell::sync::OnceCell;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::util::cosmetics_proxy::CosmeticsProxyError;

/// Audience claim Nexus's user-facing routes enforce.
const NEXUS_AUDIENCE: &str = "earth-nexus";
/// How long Nexus JWTs are good for. Kept short — they're minted per call.
const NEXUS_JWT_TTL_SECS: u64 = 120;

static HTTP_CLIENT: OnceCell<Client> = OnceCell::new();
static ENCODING_KEY: OnceCell<EncodingKey> = OnceCell::new();

fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("build reqwest client")
    })
}

fn encoding_key(private_key_path: &str) -> Result<&'static EncodingKey, CosmeticsProxyError> {
    if let Some(k) = ENCODING_KEY.get() {
        return Ok(k);
    }
    let pem = std::fs::read_to_string(private_key_path)
        .map_err(|e| CosmeticsProxyError::KeyLoadError(format!("read {private_key_path}: {e}")))?;
    let key = EncodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|e| CosmeticsProxyError::KeyLoadError(format!("parse private PEM: {e}")))?;
    let _ = ENCODING_KEY.set(key);
    Ok(ENCODING_KEY.get().expect("just set"))
}

#[derive(Serialize)]
struct NexusClaims<'a> {
    sub: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

fn mint_nexus_jwt(user_id: &str, private_key_path: &str) -> Result<String, CosmeticsProxyError> {
    let key = encoding_key(private_key_path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let claims = NexusClaims {
        sub: user_id,
        aud: NEXUS_AUDIENCE,
        iat: now,
        exp: now + NEXUS_JWT_TTL_SECS,
    };
    encode(&Header::new(Algorithm::RS256), &claims, key)
        .map_err(|e| CosmeticsProxyError::SignError(e.to_string()))
}

/// What Nexus's `/subscriptions/me` returns (the narrow subset we use).
#[derive(Debug, Deserialize)]
struct NexusSubscriptionResponse {
    subscription: Option<NexusSubscriptionRow>,
}

#[derive(Debug, Deserialize)]
struct NexusSubscriptionRow {
    tier: String,
    #[allow(dead_code)]
    status: String,
    #[serde(default)]
    current_period_end: Option<chrono::DateTime<chrono::Utc>>,
    #[allow(dead_code)]
    #[serde(default)]
    cancel_at_period_end: bool,
}

/// Possible outcomes of a refresh; the caller uses these to decide what
/// to write back to the user document.
pub enum SubscriptionRefreshOutcome {
    /// Nexus says the user has an active or past_due subscription at this tier.
    Active {
        tier: String,
        expires_at_unix: Option<i64>,
    },
    /// Nexus has no active subscription for this user — caller should
    /// reset `user.subscription` to Free.
    None,
    /// Skipped because the integration isn't configured (no base URL).
    Disabled,
}

/// Fetch the user's current Nexus subscription. Returns `Disabled` if
/// `config.nexus.base_url` is empty (development without Nexus running).
///
/// On HTTP / parse error, returns a CosmeticsProxyError variant — caller
/// should log + skip the local cache update so a transient Nexus outage
/// doesn't downgrade an entire user base to Free.
pub async fn fetch_subscription_for_user(
    user_id: &str,
) -> Result<SubscriptionRefreshOutcome, CosmeticsProxyError> {
    let cfg = config().await;
    let base = cfg.nexus.base_url.trim_end_matches('/');
    if base.is_empty() {
        return Ok(SubscriptionRefreshOutcome::Disabled);
    }
    let jwt = mint_nexus_jwt(user_id, &cfg.cosmetics.private_key_path)?;
    let res = http_client()
        .get(format!("{base}/subscriptions/me"))
        .bearer_auth(&jwt)
        .send()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(e.to_string()))?;

    let status = res.status();
    if !status.is_success() {
        let body: serde_json::Value = res
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({ "raw": "unparseable" }));
        return Err(CosmeticsProxyError::BackendError(status, body));
    }

    let parsed: NexusSubscriptionResponse = res
        .json()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(format!("parse: {e}")))?;

    Ok(match parsed.subscription {
        Some(row) => SubscriptionRefreshOutcome::Active {
            tier: row.tier,
            expires_at_unix: row.current_period_end.map(|d| d.timestamp()),
        },
        None => SubscriptionRefreshOutcome::None,
    })
}

/// Refresh `user.subscription` from Nexus and write the result back to
/// MongoDB. Logs and swallows errors — this is fire-and-forget by design
/// (called from a tokio::spawn in the request-level staleness check, and
/// awaited only by the explicit `POST /users/me/subscription-refresh`).
pub async fn refresh_user_subscription_from_nexus(db: &Database, user_id: &str) {
    use company_database::mongodb::bson::doc;

    match fetch_subscription_for_user(user_id).await {
        Ok(SubscriptionRefreshOutcome::Active { tier, expires_at_unix }) => {
            let tier_value = nexus_tier_to_company(&tier);
            let mongo = db.mongodb();
            let col = mongo.col::<company_database::mongodb::bson::Document>("users");
            let mut sub_doc = doc! { "tier": &tier_value };
            if let Some(t) = expires_at_unix {
                sub_doc.insert("expires_at", t);
            }
            let now_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            sub_doc.insert("last_nexus_refresh_at", now_unix);
            if let Err(e) = col
                .update_one(
                    doc! { "_id": user_id },
                    doc! { "$set": { "subscription": sub_doc } },
                )
                .await
            {
                log::warn!("nexus subscription mirror update failed for {user_id}: {e}");
            }
        }
        Ok(SubscriptionRefreshOutcome::None) => {
            let mongo = db.mongodb();
            let col = mongo.col::<company_database::mongodb::bson::Document>("users");
            let now_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            // Preserve customer/subscription ids so a returning subscriber
            // doesn't get a fresh Stripe customer on next checkout. Tier
            // back to Free, expires_at cleared.
            if let Err(e) = col
                .update_one(
                    doc! { "_id": user_id },
                    doc! { "$set": {
                        "subscription.tier": "Free",
                        "subscription.last_nexus_refresh_at": now_unix,
                    }, "$unset": {
                        "subscription.expires_at": "",
                    } },
                )
                .await
            {
                log::warn!("nexus subscription clear failed for {user_id}: {e}");
            }
        }
        Ok(SubscriptionRefreshOutcome::Disabled) => {
            // Nexus not configured — silently no-op. Operator can enable
            // by setting config.nexus.base_url.
        }
        Err(e) => {
            log::warn!("nexus subscription fetch failed for {user_id}: {:?}", e);
        }
    }
}

/// Map Nexus's tier strings (lowercase: "basic"|"pro"|"ultra") to
/// Company's SubscriptionTier serialization (TitleCase via serde
/// rename — see core/database/src/models/users/model.rs::SubscriptionTier).
fn nexus_tier_to_company(tier: &str) -> &'static str {
    match tier {
        "basic" => "Basic",
        "pro" => "Pro",
        "ultra" => "Ultra",
        _ => "Free",
    }
}

// ---------------------------------------------------------------------------
// Coin transfers
//
// Nexus owns the `coins` ledger and the `/transfer` endpoint is the
// atomic two-sided write — it checks the sender's balance, takes the
// platform cut (locked at 40% server-side), and credits the recipient
// in a single transaction. Anything on the Company side that moves
// coins between users (decoration purchases today, tip flow soon) goes
// through here so we never end up with a state where one side moved
// and the other didn't.
//
// The buyer's JWT must have `sub = from_user_id` — Nexus rejects
// transfers where the authenticated subject doesn't match the sender,
// so a compromised user account can't drain someone else's coins.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TransferRequestBody<'a> {
    from_user_id: &'a str,
    to_user_id: &'a str,
    coins: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    idempotency_key: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct TransferResponse {
    #[allow(dead_code)]
    #[serde(default)]
    pub transfer_id: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    pub from_balance: Option<i64>,
    #[allow(dead_code)]
    #[serde(default)]
    pub to_balance: Option<i64>,
    #[allow(dead_code)]
    #[serde(default)]
    pub platform_fee: Option<u32>,
}

/// Move `coins` from `from_user_id` to `to_user_id` via Nexus.
///
/// `idempotency_key` is forwarded as-is — Nexus uses it to dedupe
/// retries, so callers should derive it from the underlying domain
/// event (e.g. `decoration-purchase:<decoration_id>:<user_id>`) rather
/// than a fresh UUID per call.
///
/// Returns:
///   * `Ok(_)` on success
///   * `Err(BackendError(402, ..))` for insufficient funds — callers
///     should surface this to the user, not log-and-swallow
///   * `Err(BackendError(409, ..))` for idempotency conflicts (same key,
///     different params) — almost always a bug in the caller
///   * `Err(HttpError | SignError | KeyLoadError | BackendError(5xx, ..))`
///     for transient / infrastructural failures
pub async fn transfer_coins(
    from_user_id: &str,
    to_user_id: &str,
    coins: u32,
    reason: Option<&str>,
    idempotency_key: &str,
) -> Result<TransferResponse, CosmeticsProxyError> {
    let cfg = config().await;
    let base = cfg.nexus.base_url.trim_end_matches('/');
    if base.is_empty() {
        // Unlike subscription refresh, we can't silently no-op here —
        // the caller is moving money on behalf of a user. Surface it.
        return Err(CosmeticsProxyError::HttpError(
            "nexus.base_url is not configured".into(),
        ));
    }
    let jwt = mint_nexus_jwt(from_user_id, &cfg.cosmetics.private_key_path)?;
    let body = TransferRequestBody {
        from_user_id,
        to_user_id,
        coins,
        reason,
        idempotency_key,
    };
    let res = http_client()
        .post(format!("{base}/transfer"))
        .bearer_auth(&jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(e.to_string()))?;

    let status = res.status();
    if !status.is_success() {
        let err_body: serde_json::Value = res
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({ "raw": "unparseable" }));
        return Err(CosmeticsProxyError::BackendError(status, err_body));
    }

    res.json::<TransferResponse>()
        .await
        .map_err(|e| CosmeticsProxyError::HttpError(format!("parse transfer response: {e}")))
}
