use company_database::{
    CreatorEarnings, Database, DecorationPurchase, DecorationStatus, User,
};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;
use ulid::Ulid;

#[derive(Debug, Serialize, JsonSchema)]
pub struct PurchaseResponse {
    pub success: bool,
    pub purchase_id: String,
}

/// # Purchase Decoration
///
/// Purchase a paid decoration. Creates a purchase record, earnings record, and updates creator balance.
/// The 60/40 revenue split is applied: 60% to creator, 40% platform fee.
#[openapi(tag = "Decorations")]
#[post("/<id>/purchase")]
pub async fn purchase_decoration(
    db: &State<Database>,
    user: User,
    id: String,
) -> Result<Json<PurchaseResponse>> {
    let decoration = db.fetch_decoration(&id).await?;

    // Must be approved
    if !matches!(decoration.status, DecorationStatus::Approved) {
        return Err(create_error!(InvalidOperation));
    }

    // Can't purchase free decorations
    if decoration.is_free || decoration.price_cents == 0 {
        return Err(create_error!(FailedValidation {
            error: "This decoration is free and does not need to be purchased".to_string()
        }));
    }

    // Can't purchase own decoration
    if decoration.creator_id == user.id {
        return Err(create_error!(FailedValidation {
            error: "You cannot purchase your own decoration".to_string()
        }));
    }

    // Check if already owned
    let already_owned = db.user_owns_decoration(&user.id, &id).await?;
    if already_owned {
        return Err(create_error!(FailedValidation {
            error: "You already own this decoration".to_string()
        }));
    }

    let now = Timestamp::now_utc();
    let purchase_id = Ulid::new().to_string();
    let earnings_id = Ulid::new().to_string();

    // Calculate revenue split: 60% creator, 40% platform
    let gross = decoration.price_cents;
    let platform_fee = (gross as f64 * 0.4).round() as u32;
    let net_to_creator = gross - platform_fee;

    // Create purchase record
    let purchase = DecorationPurchase {
        id: purchase_id.clone(),
        user_id: user.id.clone(),
        decoration_id: id.clone(),
        price_paid_cents: gross,
        purchased_at: now,
    };

    db.insert_decoration_purchase(&purchase).await?;

    // Create earnings record
    let earnings = CreatorEarnings {
        id: earnings_id,
        creator_id: decoration.creator_id.clone(),
        decoration_id: id.clone(),
        sale_id: purchase_id.clone(),
        gross_amount_cents: gross,
        platform_fee_cents: platform_fee,
        net_amount_cents: net_to_creator,
        created_at: now,
    };

    db.insert_creator_earnings(&earnings).await?;

    // Update creator balance
    db.add_to_creator_balance(&decoration.creator_id, net_to_creator as i64)
        .await?;

    // Increment download count
    db.increment_decoration_counter(&id, "download_count")
        .await?;

    Ok(Json(PurchaseResponse {
        success: true,
        purchase_id,
    }))
}
