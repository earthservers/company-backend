use company_database::mongodb::bson::doc;
use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Subscription;

#[derive(Debug, Serialize, JsonSchema)]
pub struct SubscriptionResponse {
    /// Current subscription tier: "Free", "Basic", "Pro", "Ultra"
    tier: String,
    /// Subscription status: "active", "past_due", "cancelled", "expired", or "none"
    status: String,
    /// Unix timestamp when subscription expires
    expires_at: Option<i64>,
    /// Whether subscription will cancel at period end
    cancel_at_period_end: bool,
    /// Stripe customer ID
    stripe_customer_id: Option<String>,
    /// Current billing period end (ISO 8601)
    current_period_end: Option<String>,
    /// AI Companion licenses owned by this user
    ai_licenses: Vec<LicenseSummary>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LicenseSummary {
    /// License key
    id: String,
    /// License status: "active", "expired", etc.
    status: String,
    /// Whether the license has been activated on hardware
    activated: bool,
    /// Expiration date (ISO 8601)
    expires: String,
    /// Bound hardware ID
    hardware_id: Option<String>,
}

/// # Get Subscription
///
/// Get the current user's subscription details and AI Companion licenses.
#[openapi(tag = "Premium")]
#[get("/subscription")]
pub async fn get_subscription(
    user: User,
    db: &State<Database>,
) -> Result<Json<SubscriptionResponse>> {
    let tier = format!("{:?}", user.active_subscription_tier());
    let expires_at = user.subscription.as_ref().and_then(|s| s.expires_at);
    let stripe_customer_id = user
        .subscription
        .as_ref()
        .and_then(|s| s.stripe_customer_id.clone());

    // Query subscriptions collection for billing details
    let mongo = db.mongodb();
    let sub_col = mongo.col::<Subscription>("subscriptions");

    let billing = sub_col
        .find_one(doc! { "user_id": &user.id, "status": { "$ne": "expired" } })
        .await
        .map_err(|_| create_error!(InternalError))?;

    let (status, cancel_at_period_end, current_period_end) = if let Some(ref b) = billing {
        (
            b.status.clone(),
            b.cancel_at_period_end,
            Some(b.current_period_end.to_rfc3339()),
        )
    } else if user.active_subscription_tier().tier_rank() > 0 {
        ("active".to_string(), false, None)
    } else {
        ("none".to_string(), false, None)
    };

    // Query AI Companion licenses from license server
    let ai_licenses = fetch_user_licenses(&user.id).await.unwrap_or_default();

    Ok(Json(SubscriptionResponse {
        tier,
        status,
        expires_at,
        cancel_at_period_end,
        stripe_customer_id,
        current_period_end,
        ai_licenses,
    }))
}

/// Fetch a user's AI Companion licenses from the license server.
async fn fetch_user_licenses(user_id: &str) -> std::result::Result<Vec<LicenseSummary>, String> {
    let license_server_url = std::env::var("LICENSE_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:14800".to_string());

    let admin_key = std::env::var("LICENSE_SERVER_ADMIN_KEY")
        .map_err(|_| "LICENSE_SERVER_ADMIN_KEY not set".to_string())?;

    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/admin/user/{}/licenses", license_server_url, user_id))
        .header("X-Admin-Key", &admin_key)
        .send()
        .await
        .map_err(|e| format!("License server request failed: {e}"))?;

    if !response.status().is_success() {
        return Ok(Vec::new());
    }

    let licenses: Vec<LicenseSummary> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse license server response: {e}"))?;

    Ok(licenses)
}
