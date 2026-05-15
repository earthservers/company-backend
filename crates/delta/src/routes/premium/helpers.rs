use company_config::Stripe;
use company_database::mongodb::bson::doc;
use company_database::Database;
use company_result::{create_error, Result};

/// Map a Stripe Price ID back to a tier name.
pub fn price_id_to_tier(price_id: &str, config: &Stripe) -> Option<&'static str> {
    if price_id == config.price_basic_monthly {
        Some("Basic")
    } else if price_id == config.price_pro_monthly {
        Some("Pro")
    } else if price_id == config.price_ultra_monthly {
        Some("Ultra")
    } else {
        None
    }
}

/// Map a product string from the checkout request to a tier name for the enum.
pub fn product_to_subscription_tier(product: &str) -> Option<&'static str> {
    match product {
        "basic" => Some("Basic"),
        "pro" => Some("Pro"),
        "ultra" => Some("Ultra"),
        _ => None,
    }
}

/// Update the user.subscription field on the users collection.
pub async fn update_user_subscription(
    db: &Database,
    user_id: &str,
    tier: &str,
    stripe_customer_id: &str,
    stripe_subscription_id: &str,
    expires_at: i64,
) -> Result<()> {
    let mongo = db.mongodb();
    let col = mongo.col::<company_database::mongodb::bson::Document>("users");

    col.update_one(
        doc! { "_id": user_id },
        doc! {
            "$set": {
                "subscription.tier": tier,
                "subscription.stripe_customer_id": stripe_customer_id,
                "subscription.stripe_subscription_id": stripe_subscription_id,
                "subscription.expires_at": expires_at,
            }
        },
    )
    .await
    .map_err(|_| create_error!(InternalError))?;

    Ok(())
}

/// Clear the user's subscription back to Free tier.
pub async fn clear_user_subscription(db: &Database, user_id: &str) -> Result<()> {
    let mongo = db.mongodb();
    let col = mongo.col::<company_database::mongodb::bson::Document>("users");

    col.update_one(
        doc! { "_id": user_id },
        doc! {
            "$set": {
                "subscription.tier": "Free",
            },
            "$unset": {
                "subscription.stripe_customer_id": "",
                "subscription.stripe_subscription_id": "",
                "subscription.expires_at": "",
            }
        },
    )
    .await
    .map_err(|_| create_error!(InternalError))?;

    Ok(())
}

/// Generate an AI Companion license for a user via the separate license server.
pub async fn generate_ai_companion_license(
    user_id: &str,
    tier: &str,
    email: &str,
    _db: &Database,
) -> Result<()> {
    // Call separate license server instead of local MongoDB
    let license_server_url = std::env::var("LICENSE_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:14800".to_string());

    let admin_key = std::env::var("LICENSE_SERVER_ADMIN_KEY")
        .map_err(|_| create_error!(InternalError))?;

    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/admin/generate", license_server_url))
        .header("X-Admin-Key", admin_key)
        .json(&serde_json::json!({
            "user_id": user_id,
            "tier": tier,
            "license_type": "companion",
            "email": email,
        }))
        .send()
        .await
        .map_err(|e| {
            eprintln!("Failed to call license server: {}", e);
            create_error!(InternalError)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        eprintln!("License server error {}: {}", status, body);
        return Err(create_error!(InternalError));
    }

    let data: serde_json::Value = response.json()
        .await
        .map_err(|e| {
            eprintln!("Failed to parse license server response: {}", e);
            create_error!(InternalError)
        })?;

    let license_key = data["license_key"]
        .as_str()
        .ok_or_else(|| {
            eprintln!("License server didn't return license_key");
            create_error!(InternalError)
        })?;

    println!("Generated license: {} for user {}", license_key, user_id);

    // TODO: Send email with license key
    // For now, just log it

    Ok(())
}
