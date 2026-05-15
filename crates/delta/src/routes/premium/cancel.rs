use company_config::Stripe;
use company_database::mongodb::bson::doc;
use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

use super::Subscription;

#[derive(Debug, Serialize, JsonSchema)]
pub struct CancelResponse {
    /// Whether the cancellation was successful
    success: bool,
    /// Human-readable message
    message: String,
    /// When the subscription will end (ISO 8601)
    cancel_at: Option<String>,
}

/// # Cancel Subscription
///
/// Cancel the current user's premium subscription at the end of the current billing period.
/// The subscription remains active until the period ends.
#[openapi(tag = "Premium")]
#[post("/cancel")]
pub async fn cancel_subscription(
    user: User,
    db: &State<Database>,
    stripe_config: &State<Stripe>,
) -> Result<Json<CancelResponse>> {
    // Get user's Stripe subscription ID
    let stripe_sub_id = user
        .subscription
        .as_ref()
        .and_then(|s| s.stripe_subscription_id.as_ref())
        .filter(|sid| !sid.is_empty())
        .ok_or_else(|| create_error!(NoActiveSubscription))?;

    // Cancel at period end via Stripe API
    let stripe_client = reqwest::Client::new();

    let response = stripe_client
        .post(format!(
            "https://api.stripe.com/v1/subscriptions/{}",
            stripe_sub_id
        ))
        .header(
            "Authorization",
            format!("Bearer {}", stripe_config.secret_key),
        )
        .form(&[("cancel_at_period_end", "true")])
        .send()
        .await
        .map_err(|e| {
            create_error!(StripeError {
                message: format!("Failed to cancel subscription: {e}")
            })
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        log::error!("Stripe cancel error: {error_text}");
        return Err(create_error!(StripeError {
            message: "Failed to cancel subscription".to_string()
        }));
    }

    let sub_data: serde_json::Value = response.json().await.map_err(|_| {
        create_error!(StripeError {
            message: "Failed to parse Stripe cancel response".to_string()
        })
    })?;

    let cancel_at = sub_data["current_period_end"]
        .as_i64()
        .map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .unwrap_or_default()
                .to_rfc3339()
        });

    // Update subscriptions collection
    let mongo = db.mongodb();
    let col = mongo.col::<Subscription>("subscriptions");

    let _ = col
        .update_one(
            doc! { "user_id": &user.id, "status": "active" },
            doc! { "$set": {
                "cancel_at_period_end": true,
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }},
        )
        .await;

    Ok(Json(CancelResponse {
        success: true,
        message: "Subscription will cancel at the end of the current billing period".to_string(),
        cancel_at,
    }))
}
