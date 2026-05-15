use chrono::Utc;
use company_config::Stripe;
use company_database::mongodb::bson::doc;
use company_database::Database;
use hmac::{Hmac, Mac};
use rocket::data::{Data, ToByteUnit};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;
use sha2::Sha256;

use super::Subscription;
use crate::routes::premium::helpers;

type HmacSha256 = Hmac<Sha256>;

/// Request guard that extracts the Stripe-Signature header.
pub struct StripeSignature(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for StripeSignature {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("Stripe-Signature") {
            Some(sig) => Outcome::Success(StripeSignature(sig.to_string())),
            None => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

/// # Stripe Webhook
///
/// Receives and processes Stripe webhook events. NOT behind user auth.
/// Verifies the Stripe-Signature header before processing.
#[post("/webhook", data = "<body>")]
pub async fn stripe_webhook(
    body: Data<'_>,
    signature: StripeSignature,
    db: &State<Database>,
    stripe_config: &State<Stripe>,
) -> Status {
    // Read raw body
    let body_bytes = match body.open(512.kibibytes()).into_bytes().await {
        Ok(bytes) if bytes.is_complete() => bytes.into_inner(),
        _ => return Status::BadRequest,
    };

    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => return Status::BadRequest,
    };

    // Verify webhook signature
    if !verify_stripe_signature(body_str, &signature.0, &stripe_config.webhook_secret) {
        log::warn!("Stripe webhook signature verification failed");
        return Status::Unauthorized;
    }

    // Parse event
    let event: serde_json::Value = match serde_json::from_str(body_str) {
        Ok(e) => e,
        Err(_) => return Status::BadRequest,
    };

    let event_type = event["type"].as_str().unwrap_or("");
    log::info!("Stripe webhook event: {event_type}");

    // Handle event
    match event_type {
        "checkout.session.completed" => {
            if let Err(e) = handle_checkout_completed(&event["data"]["object"], db, stripe_config).await {
                log::error!("Error handling checkout.session.completed: {e}");
                return Status::InternalServerError;
            }
        }
        "customer.subscription.updated" => {
            if let Err(e) = handle_subscription_updated(&event["data"]["object"], db, stripe_config).await {
                log::error!("Error handling customer.subscription.updated: {e}");
                return Status::InternalServerError;
            }
        }
        "customer.subscription.deleted" => {
            if let Err(e) = handle_subscription_deleted(&event["data"]["object"], db).await {
                log::error!("Error handling customer.subscription.deleted: {e}");
                return Status::InternalServerError;
            }
        }
        "invoice.payment_failed" => {
            if let Err(e) = handle_payment_failed(&event["data"]["object"], db).await {
                log::error!("Error handling invoice.payment_failed: {e}");
                return Status::InternalServerError;
            }
        }
        _ => {
            log::debug!("Unhandled Stripe webhook event type: {event_type}");
        }
    }

    Status::Ok
}

/// Verify Stripe webhook signature using HMAC-SHA256.
fn verify_stripe_signature(payload: &str, signature_header: &str, secret: &str) -> bool {
    // Parse the signature header: "t=TIMESTAMP,v1=SIGNATURE,..."
    let mut timestamp = "";
    let mut expected_sig = "";

    for part in signature_header.split(',') {
        if let Some(t) = part.strip_prefix("t=") {
            timestamp = t;
        } else if let Some(v) = part.strip_prefix("v1=") {
            expected_sig = v;
        }
    }

    if timestamp.is_empty() || expected_sig.is_empty() {
        return false;
    }

    // Compute expected signature: HMAC-SHA256(secret, "timestamp.payload")
    let signed_payload = format!("{timestamp}.{payload}");

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(signed_payload.as_bytes());

    let result = mac.finalize().into_bytes();
    let computed_sig: String = result.iter().map(|b| format!("{b:02x}")).collect();

    // Constant-time comparison
    computed_sig == expected_sig
}

/// Handle checkout.session.completed: create subscription record or generate license.
async fn handle_checkout_completed(
    session: &serde_json::Value,
    db: &State<Database>,
    stripe_config: &State<Stripe>,
) -> std::result::Result<(), String> {
    let user_id = session["client_reference_id"]
        .as_str()
        .or_else(|| session["metadata"]["user_id"].as_str())
        .ok_or("No user_id in session")?;

    let product = session["metadata"]["product"]
        .as_str()
        .unwrap_or("unknown");

    let customer_id = session["customer"]
        .as_str()
        .unwrap_or_default();

    if product == "decoration_submission" {
        // One-time purchase: create decoration in DB
        let name = session["metadata"]["decoration_name"]
            .as_str()
            .unwrap_or("Untitled");
        let description = session["metadata"]["decoration_description"]
            .as_str()
            .unwrap_or("");
        let asset_id = session["metadata"]["decoration_asset_id"]
            .as_str()
            .unwrap_or_default();
        let asset_type = session["metadata"]["decoration_asset_type"]
            .as_str()
            .unwrap_or("png");
        let category_str = session["metadata"]["decoration_category"]
            .as_str()
            .unwrap_or("Badge");

        let category = match category_str {
            "AvatarFrame" => company_database::DecorationCategory::AvatarFrame,
            "Banner" => company_database::DecorationCategory::Banner,
            "Nameplate" => company_database::DecorationCategory::Nameplate,
            _ => company_database::DecorationCategory::Badge,
        };

        let decoration = company_database::Decoration {
            id: ulid::Ulid::new().to_string(),
            creator_id: user_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            asset_id: Some(asset_id.to_string()),
            asset_type: Some(asset_type.to_string()),
            lottie_json: None,
            category,
            status: company_database::DecorationStatus::Pending,
            canvas_width: 0,
            canvas_height: 0,
            duration_seconds: 0.0,
            fps: 0,
            is_free: false,
            price_cents: 0,
            creator_wants_free: false,
            thumbnail_url: None,
            download_count: 0,
            active_users_count: 0,
            created_at: iso8601_timestamp::Timestamp::now_utc(),
            approved_at: None,
            approved_by: None,
            rejected_reason: None,
            moderator_notes: None,
        };

        db.insert_decoration(&decoration)
            .await
            .map_err(|e| format!("Failed to insert decoration: {e}"))?;

        log::info!("Created decoration '{}' for user {} after payment", name, user_id);
    } else if product == "ai_companion" {
        // One-time purchase: generate license
        // Get user email from the session or use empty
        let email = session["customer_details"]["email"]
            .as_str()
            .unwrap_or_default();

        if let Err(e) = helpers::generate_ai_companion_license(user_id, "ai_companion", email, db).await {
            return Err(format!("Failed to generate license: {e}"));
        }
        log::info!("Generated AI Companion license for user {user_id}");
    } else {
        // Subscription: create billing record
        let stripe_sub_id = session["subscription"]
            .as_str()
            .unwrap_or_default();

        // Determine tier from product metadata
        let tier = helpers::product_to_subscription_tier(product)
            .unwrap_or("Basic");

        // Fetch the subscription from Stripe to get period details
        let period_end = fetch_subscription_period_end(stripe_sub_id, stripe_config)
            .await
            .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

        let now = Utc::now();
        let sub_id = ulid::Ulid::new().to_string();

        let subscription = Subscription {
            id: sub_id,
            user_id: user_id.to_string(),
            tier: tier.to_string(),
            status: "active".to_string(),
            stripe_customer_id: customer_id.to_string(),
            stripe_subscription_id: stripe_sub_id.to_string(),
            stripe_price_id: String::new(),
            current_period_start: now,
            current_period_end: period_end,
            cancel_at_period_end: false,
            created_at: now,
            updated_at: now,
        };

        let mongo = db.mongodb();
        let col = mongo.col::<Subscription>("subscriptions");

        // Upsert to handle duplicate webhook deliveries
        col.update_one(
            doc! { "user_id": user_id, "stripe_subscription_id": stripe_sub_id },
            doc! { "$set": company_database::mongodb::bson::to_bson(&subscription)
                .map_err(|e| format!("BSON serialization error: {e}"))?
            },
        )
        .with_options(
            company_database::mongodb::options::UpdateOptions::builder()
                .upsert(true)
                .build(),
        )
        .await
        .map_err(|e| format!("Failed to upsert subscription: {e}"))?;

        // Update user.subscription (denormalized cache)
        let expires_at = period_end.timestamp();
        if let Err(e) = helpers::update_user_subscription(
            db,
            user_id,
            tier,
            customer_id,
            stripe_sub_id,
            expires_at,
        )
        .await
        {
            log::error!("Failed to update user subscription cache: {e}");
        }

        log::info!("Created {tier} subscription for user {user_id}");
    }

    Ok(())
}

/// Handle customer.subscription.updated: sync tier/status changes.
async fn handle_subscription_updated(
    sub_data: &serde_json::Value,
    db: &State<Database>,
    stripe_config: &State<Stripe>,
) -> std::result::Result<(), String> {
    let stripe_sub_id = sub_data["id"].as_str().unwrap_or_default();
    let status = sub_data["status"].as_str().unwrap_or("active");
    let cancel_at_period_end = sub_data["cancel_at_period_end"].as_bool().unwrap_or(false);

    let period_end = sub_data["current_period_end"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(Utc::now);

    // Determine tier from price ID in items
    let price_id = sub_data["items"]["data"][0]["price"]["id"]
        .as_str()
        .unwrap_or_default();

    let tier = helpers::price_id_to_tier(price_id, stripe_config)
        .unwrap_or("Basic");

    let mongo = db.mongodb();
    let col = mongo.col::<Subscription>("subscriptions");

    // Update subscription record
    col.update_one(
        doc! { "stripe_subscription_id": stripe_sub_id },
        doc! { "$set": {
            "tier": tier,
            "status": status,
            "cancel_at_period_end": cancel_at_period_end,
            "current_period_end": period_end.to_rfc3339(),
            "updated_at": Utc::now().to_rfc3339(),
        }},
    )
    .await
    .map_err(|e| format!("Failed to update subscription: {e}"))?;

    // Find the associated user
    let sub = col
        .find_one(doc! { "stripe_subscription_id": stripe_sub_id })
        .await
        .map_err(|e| format!("Failed to find subscription: {e}"))?;

    if let Some(sub) = sub {
        let db_status = match status {
            "active" | "trialing" => {
                // Update user tier
                if let Err(e) = helpers::update_user_subscription(
                    db,
                    &sub.user_id,
                    tier,
                    &sub.stripe_customer_id,
                    stripe_sub_id,
                    period_end.timestamp(),
                )
                .await
                {
                    log::error!("Failed to update user subscription: {e}");
                }
                "active"
            }
            "past_due" => "past_due",
            "canceled" | "unpaid" => {
                // Downgrade to free
                if let Err(e) = helpers::clear_user_subscription(db, &sub.user_id).await {
                    log::error!("Failed to clear user subscription: {e}");
                }
                "cancelled"
            }
            _ => status,
        };

        log::info!(
            "Subscription {} updated: status={db_status}, tier={tier}, cancel_at_period_end={cancel_at_period_end}",
            stripe_sub_id
        );
    }

    Ok(())
}

/// Handle customer.subscription.deleted: mark as expired and downgrade user.
async fn handle_subscription_deleted(
    sub_data: &serde_json::Value,
    db: &State<Database>,
) -> std::result::Result<(), String> {
    let stripe_sub_id = sub_data["id"].as_str().unwrap_or_default();

    let mongo = db.mongodb();
    let col = mongo.col::<Subscription>("subscriptions");

    // Update subscription status
    col.update_one(
        doc! { "stripe_subscription_id": stripe_sub_id },
        doc! { "$set": {
            "status": "expired",
            "updated_at": Utc::now().to_rfc3339(),
        }},
    )
    .await
    .map_err(|e| format!("Failed to update subscription: {e}"))?;

    // Find and downgrade user
    let sub = col
        .find_one(doc! { "stripe_subscription_id": stripe_sub_id })
        .await
        .map_err(|e| format!("Failed to find subscription: {e}"))?;

    if let Some(sub) = sub {
        if let Err(e) = helpers::clear_user_subscription(db, &sub.user_id).await {
            log::error!("Failed to clear user subscription: {e}");
        }
        log::info!("Subscription deleted for user {}, downgraded to Free", sub.user_id);
    }

    Ok(())
}

/// Handle invoice.payment_failed: mark subscription as past_due.
async fn handle_payment_failed(
    invoice: &serde_json::Value,
    db: &State<Database>,
) -> std::result::Result<(), String> {
    let stripe_sub_id = invoice["subscription"]
        .as_str()
        .unwrap_or_default();

    if stripe_sub_id.is_empty() {
        return Ok(()); // Not a subscription invoice
    }

    let mongo = db.mongodb();
    let col = mongo.col::<Subscription>("subscriptions");

    col.update_one(
        doc! { "stripe_subscription_id": stripe_sub_id },
        doc! { "$set": {
            "status": "past_due",
            "updated_at": Utc::now().to_rfc3339(),
        }},
    )
    .await
    .map_err(|e| format!("Failed to update subscription status: {e}"))?;

    log::warn!("Payment failed for subscription {stripe_sub_id}");

    Ok(())
}

/// Fetch subscription period end from Stripe API.
async fn fetch_subscription_period_end(
    sub_id: &str,
    stripe_config: &State<Stripe>,
) -> Option<chrono::DateTime<Utc>> {
    if sub_id.is_empty() {
        return None;
    }

    let client = reqwest::Client::new();
    let response = client
        .get(format!("https://api.stripe.com/v1/subscriptions/{sub_id}"))
        .header(
            "Authorization",
            format!("Bearer {}", stripe_config.secret_key),
        )
        .send()
        .await
        .ok()?;

    let data: serde_json::Value = response.json().await.ok()?;
    data["current_period_end"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
}
