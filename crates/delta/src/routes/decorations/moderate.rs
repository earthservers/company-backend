use company_database::{Database, User};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Deserialize;

use super::DecorationResponse;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModerateDecorationRequest {
    /// New status: "Approved" or "Rejected"
    pub status: String,
    /// Reason for rejection (optional, used when rejecting)
    pub reason: Option<String>,
    /// Moderator notes (internal)
    pub moderator_notes: Option<String>,
    /// Override price in cents (moderator can set final price)
    pub price_cents: Option<u32>,
    /// Override free status (moderator can make it free or paid)
    pub is_free: Option<bool>,
}

/// # Moderate Decoration
///
/// Approve or reject a pending decoration. Requires privileged user.
/// Moderator can set final pricing, override free/paid status, and add notes.
#[openapi(tag = "Decorations")]
#[patch("/<id>/moderate", data = "<data>")]
pub async fn moderate_decoration(
    db: &State<Database>,
    user: User,
    id: String,
    data: Json<ModerateDecorationRequest>,
) -> Result<Json<DecorationResponse>> {
    // Require privileged user
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let data = data.into_inner();

    // Validate status
    if !["Approved", "Rejected"].contains(&data.status.as_str()) {
        return Err(create_error!(FailedValidation {
            error: "Status must be 'Approved' or 'Rejected'".to_string()
        }));
    }

    let approved_at = if data.status == "Approved" {
        Some(Timestamp::now_utc())
    } else {
        None
    };

    let approved_by = if data.status == "Approved" {
        Some(user.id.as_str())
    } else {
        None
    };

    db.update_decoration_status(
        &id,
        &data.status,
        data.reason.as_deref(),
        approved_at,
        approved_by,
        data.moderator_notes.as_deref(),
        data.price_cents,
        data.is_free,
    )
    .await?;

    // Fetch and return the updated decoration
    let decoration = db.fetch_decoration(&id).await?;
    Ok(Json(decoration.into()))
}
