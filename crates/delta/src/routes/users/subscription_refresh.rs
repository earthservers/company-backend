use company_database::{Database, User};
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;
use serde::Serialize;

use crate::util::nexus::refresh_user_subscription_from_nexus;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SubscriptionRefreshResponse {
    pub refreshed: bool,
}

/// # Refresh Subscription from Nexus
///
/// Force a synchronous refresh of the caller's `user.subscription`
/// from Earth Nexus. Intended to be called by the frontend immediately
/// after a successful Stripe Checkout redirect (e.g. when the URL
/// returns with `?subscribed=1`) so the new tier is reflected without
/// waiting for the next periodic background refresh.
///
/// Returns `refreshed: false` if Nexus is unreachable / not configured;
/// the request still 200s because the background refresh on subsequent
/// requests will catch up. Failure is observable but not blocking.
#[openapi(tag = "User Information")]
#[post("/@me/subscription-refresh")]
pub async fn refresh(
    user: User,
    db: &State<Database>,
) -> Result<Json<SubscriptionRefreshResponse>> {
    refresh_user_subscription_from_nexus(db.inner(), &user.id).await;
    // We don't currently surface refresh failures here because the function
    // logs them and the next-request periodic refresh is the safety net.
    // Returning {refreshed: true} on best-effort completion is enough for
    // the frontend to know to re-fetch /users/@me.
    Ok(Json(SubscriptionRefreshResponse { refreshed: true }))
}
