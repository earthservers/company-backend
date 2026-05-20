use company_database::{Database, User};
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;

use super::BeaconResponse;

/// # List My Beacons
///
/// Returns every non-deleted beacon owned by the authenticated user,
/// newest first. Used by the management UI to render the share ledger.
#[openapi(tag = "Beacons")]
#[get("/mine")]
pub async fn list_my_beacons(
    db: &State<Database>,
    user: User,
) -> Result<Json<Vec<BeaconResponse>>> {
    let rows = db.fetch_beacons_by_owner(&user.id).await?;
    Ok(Json(rows.into_iter().map(|b| b.into()).collect()))
}
