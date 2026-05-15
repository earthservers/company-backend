use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;

use super::DecorationResponse;

/// # List Pending Decorations
///
/// Dedicated endpoint for moderators to fetch pending decorations.
/// Requires privileged user.
#[openapi(tag = "Decorations")]
#[get("/pending?<limit>&<offset>")]
pub async fn list_pending_decorations(
    db: &State<Database>,
    user: User,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Json<Vec<DecorationResponse>>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let limit = limit.unwrap_or(100).min(200).max(1);
    let offset = offset.unwrap_or(0).max(0);

    let decorations = db
        .fetch_decorations_by_status("Pending", None, None, limit, offset)
        .await?;

    Ok(Json(decorations.into_iter().map(|d| d.into()).collect()))
}
