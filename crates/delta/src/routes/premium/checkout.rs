use company_config::Stripe;
use company_database::mongodb::bson::doc;
use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckoutRequest {
    /// Product to purchase: "basic", "pro", "ultra", or "ai_companion"
    product: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CheckoutResponse {
    /// Stripe Checkout Session URL to redirect the user to
    url: String,
    /// Checkout Session ID
    session_id: String,
}

/// # Create Checkout Session
///
/// Create a Stripe Checkout Session for a premium subscription or AI Companion license purchase.
/// Returns a URL to redirect the user to.
#[openapi(tag = "Premium")]
#[post("/checkout", data = "<req>")]
pub async fn create_checkout(
    user: User,
    req: Json<CheckoutRequest>,
    db: &State<Database>,
    stripe_config: &State<Stripe>,
) -> Result<Json<CheckoutResponse>> {
    // Determine price ID and checkout mode
    let (price_id, is_subscription) = match req.product.as_str() {
        "basic" => (&stripe_config.price_basic_monthly, true),
        "pro" => (&stripe_config.price_pro_monthly, true),
        "ultra" => (&stripe_config.price_ultra_monthly, true),
        "ai_companion" => (&stripe_config.price_ai_companion, false),
        _ => return Err(create_error!(InvalidOperation)),
    };

    if price_id.is_empty() {
        return Err(create_error!(StripeError {
            message: "Price ID not configured for this product".to_string()
        }));
    }

    // For subscriptions, check if user already has an equal or higher tier
    if is_subscription {
        let target_rank = match req.product.as_str() {
            "basic" => 1u8,
            "pro" => 2,
            "ultra" => 3,
            _ => 0,
        };
        let current_rank = user.active_subscription_tier().tier_rank();
        if current_rank >= target_rank {
            return Err(create_error!(AlreadySubscribed));
        }
    }

    // Look up or create Stripe customer
    let customer_id = get_or_create_customer(&user, db, stripe_config).await?;

    // Create Stripe Checkout Session via API
    let stripe_client = reqwest::Client::new();

    let mode = if is_subscription {
        "subscription"
    } else {
        "payment"
    };

    let success_url = format!(
        "{}&session_id={{CHECKOUT_SESSION_ID}}",
        stripe_config.success_url
    );

    let params = vec![
        ("mode", mode.to_string()),
        ("customer", customer_id.clone()),
        ("line_items[0][price]", price_id.clone()),
        ("line_items[0][quantity]", "1".to_string()),
        ("success_url", success_url),
        ("cancel_url", stripe_config.cancel_url.clone()),
        ("client_reference_id", user.id.clone()),
        ("metadata[user_id]", user.id.clone()),
        ("metadata[product]", req.product.clone()),
    ];

    let response = stripe_client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .header(
            "Authorization",
            format!("Bearer {}", stripe_config.secret_key),
        )
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            create_error!(StripeError {
                message: format!("Failed to create checkout session: {e}")
            })
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        log::error!("Stripe checkout error: {error_text}");
        return Err(create_error!(StripeError {
            message: "Failed to create checkout session".to_string()
        }));
    }

    let session: serde_json::Value = response.json().await.map_err(|_| {
        create_error!(StripeError {
            message: "Failed to parse Stripe response".to_string()
        })
    })?;

    let url = session["url"]
        .as_str()
        .ok_or_else(|| {
            create_error!(StripeError {
                message: "No URL in Stripe response".to_string()
            })
        })?
        .to_string();

    let session_id = session["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    Ok(Json(CheckoutResponse { url, session_id }))
}

/// Get existing Stripe customer ID or create a new one.
async fn get_or_create_customer(
    user: &User,
    db: &State<Database>,
    stripe_config: &State<Stripe>,
) -> Result<String> {
    // Check if user already has a Stripe customer ID
    if let Some(ref sub) = user.subscription {
        if let Some(ref cid) = sub.stripe_customer_id {
            if !cid.is_empty() {
                return Ok(cid.clone());
            }
        }
    }

    // Create new Stripe customer
    let stripe_client = reqwest::Client::new();

    let params = vec![
        ("metadata[user_id]", user.id.clone()),
    ];

    let response = stripe_client
        .post("https://api.stripe.com/v1/customers")
        .header(
            "Authorization",
            format!("Bearer {}", stripe_config.secret_key),
        )
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            create_error!(StripeError {
                message: format!("Failed to create Stripe customer: {e}")
            })
        })?;

    if !response.status().is_success() {
        return Err(create_error!(StripeError {
            message: "Failed to create Stripe customer".to_string()
        }));
    }

    let customer: serde_json::Value = response.json().await.map_err(|_| {
        create_error!(StripeError {
            message: "Failed to parse Stripe customer response".to_string()
        })
    })?;

    let customer_id = customer["id"]
        .as_str()
        .ok_or_else(|| {
            create_error!(StripeError {
                message: "No customer ID in Stripe response".to_string()
            })
        })?
        .to_string();

    // Store customer ID on user record
    let mongo = db.mongodb();
    let col = mongo.col::<company_database::mongodb::bson::Document>("users");
    let _ = col
        .update_one(
            doc! { "_id": &user.id },
            doc! { "$set": { "subscription.stripe_customer_id": &customer_id } },
        )
        .await;

    Ok(customer_id)
}
