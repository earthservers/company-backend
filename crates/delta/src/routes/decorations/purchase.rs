use company_database::{Database, DecorationPurchase, DecorationStatus, User};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;
use ulid::Ulid;

use crate::util::cosmetics_proxy::CosmeticsProxyError;
use crate::util::nexus::transfer_coins;

#[derive(Debug, Serialize, JsonSchema)]
pub struct PurchaseResponse {
    pub success: bool,
    pub purchase_id: String,
}

/// # Purchase Decoration
///
/// Buy a paid decoration. The coin movement itself happens in **Earth
/// Nexus** (the central ledger) via `/transfer`; this route is just the
/// ownership-recording side. After Nexus confirms the transfer we
/// insert a `DecorationPurchase` row + bump the decoration's download
/// count. The 60/40 platform cut is enforced server-side inside Nexus —
/// we don't see (or need to see) the gross/net split here.
///
/// Idempotency: the key sent to Nexus is `decoration-purchase:<deco>:<user>`,
/// so a client retry can't double-charge — Nexus will return the same
/// transfer the second time without moving money again.
#[openapi(tag = "Decorations")]
#[post("/<id>/purchase")]
pub async fn purchase_decoration(
    db: &State<Database>,
    user: User,
    id: String,
) -> Result<Json<PurchaseResponse>> {
    let decoration = db.fetch_decoration(&id).await?;

    if !matches!(decoration.status, DecorationStatus::Approved) {
        return Err(create_error!(InvalidOperation));
    }

    let coins = decoration.price_coins;

    if decoration.is_free || coins == 0 {
        return Err(create_error!(FailedValidation {
            error: "This decoration is free and does not need to be purchased".to_string()
        }));
    }

    if decoration.creator_id == user.id {
        return Err(create_error!(FailedValidation {
            error: "You cannot purchase your own decoration".to_string()
        }));
    }

    let already_owned = db.user_owns_decoration(&user.id, &id).await?;
    if already_owned {
        return Err(create_error!(FailedValidation {
            error: "You already own this decoration".to_string()
        }));
    }

    // Move the coins via Nexus before recording ownership — if Nexus
    // rejects (insufficient funds, transient failure, etc.) we bail
    // without granting the decoration.
    let idempotency_key = format!("decoration-purchase:{}:{}", id, user.id);
    let reason = format!("decoration:{}", id);
    match transfer_coins(
        &user.id,
        &decoration.creator_id,
        coins,
        Some(&reason),
        &idempotency_key,
    )
    .await
    {
        Ok(_) => {}
        Err(CosmeticsProxyError::BackendError(status, body)) => {
            log::warn!(
                "nexus /transfer rejected decoration purchase {} for {}: {} {}",
                id,
                user.id,
                status,
                body
            );
            // Surface the typical user-facing failures (insufficient
            // funds = 402, validation = 400) as FailedValidation so the
            // client can show the message; treat anything else (auth,
            // 5xx, etc.) as InternalError.
            let code = status.as_u16();
            if code == 400 || code == 402 || code == 409 {
                let message = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .or_else(|| body.get("error").and_then(|v| v.as_str()))
                    .unwrap_or("Coin transfer failed")
                    .to_string();
                return Err(create_error!(FailedValidation { error: message }));
            }
            return Err(create_error!(InternalError));
        }
        Err(e) => {
            log::warn!(
                "nexus /transfer infrastructural failure for decoration {} user {}: {:?}",
                id,
                user.id,
                e
            );
            return Err(create_error!(InternalError));
        }
    }

    // From here on the coins are gone — record ownership. Any failure
    // below is recoverable via support (the transfer is logged in
    // Nexus + the idempotency key is deterministic, so a replay is safe).
    let now = Timestamp::now_utc();
    let purchase_id = Ulid::new().to_string();

    let purchase = DecorationPurchase {
        id: purchase_id.clone(),
        user_id: user.id.clone(),
        decoration_id: id.clone(),
        price_paid_coins: coins,
        purchased_at: now,
    };

    db.insert_decoration_purchase(&purchase).await?;

    db.increment_decoration_counter(&id, "download_count")
        .await?;

    Ok(Json(PurchaseResponse {
        success: true,
        purchase_id,
    }))
}
