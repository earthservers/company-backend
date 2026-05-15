use company_config::Stripe;
use company_database::User;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct PortalResponse {
    /// Stripe Customer Portal URL
    url: String,
}

/// # Create Customer Portal Session
///
/// Create a Stripe Customer Portal session for managing billing, payment methods, and invoices.
#[openapi(tag = "Premium")]
#[post("/portal")]
pub async fn create_portal(
    user: User,
    stripe_config: &State<Stripe>,
) -> Result<Json<PortalResponse>> {
    // Get user's Stripe customer ID
    let customer_id = user
        .subscription
        .as_ref()
        .and_then(|s| s.stripe_customer_id.as_ref())
        .filter(|cid| !cid.is_empty())
        .ok_or_else(|| create_error!(NoActiveSubscription))?;

    // Create Stripe Billing Portal session
    let stripe_client = reqwest::Client::new();

    let params = vec![
        ("customer", customer_id.as_str()),
        ("return_url", stripe_config.cancel_url.as_str()),
    ];

    let response = stripe_client
        .post("https://api.stripe.com/v1/billing_portal/sessions")
        .header(
            "Authorization",
            format!("Bearer {}", stripe_config.secret_key),
        )
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            create_error!(StripeError {
                message: format!("Failed to create portal session: {e}")
            })
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        log::error!("Stripe portal error: {error_text}");
        return Err(create_error!(StripeError {
            message: "Failed to create portal session".to_string()
        }));
    }

    let session: serde_json::Value = response.json().await.map_err(|_| {
        create_error!(StripeError {
            message: "Failed to parse Stripe portal response".to_string()
        })
    })?;

    let url = session["url"]
        .as_str()
        .ok_or_else(|| {
            create_error!(StripeError {
                message: "No URL in Stripe portal response".to_string()
            })
        })?
        .to_string();

    Ok(Json(PortalResponse { url }))
}
