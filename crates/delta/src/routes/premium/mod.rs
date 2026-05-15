use chrono::{DateTime, Utc};
use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;
use serde::{Deserialize, Serialize};

pub mod cancel;
pub mod checkout;
mod helpers;
pub mod limits;
pub mod portal;
pub mod subscription;
pub mod webhook;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        checkout::create_checkout,
        portal::create_portal,
        subscription::get_subscription,
        limits::get_limits,
        cancel::cancel_subscription,
    ]
}

/// Webhook route is NOT part of the OpenAPI spec (no user auth).
/// Must be mounted separately via `rocket.mount()`.
pub fn webhook_routes() -> Vec<Route> {
    routes![webhook::stripe_webhook]
}

/// A billing subscription record in the "subscriptions" collection.
/// Source-of-truth for billing state; the user.subscription field
/// is a denormalized cache updated by webhook handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    #[serde(rename = "_id")]
    pub id: String,
    /// FK -> users._id
    pub user_id: String,
    /// "basic", "pro", "ultra"
    pub tier: String,
    /// active, past_due, cancelled, expired
    pub status: String,
    pub stripe_customer_id: String,
    pub stripe_subscription_id: String,
    pub stripe_price_id: String,
    pub current_period_start: DateTime<Utc>,
    pub current_period_end: DateTime<Utc>,
    pub cancel_at_period_end: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
