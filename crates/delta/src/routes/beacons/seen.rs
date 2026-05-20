use company_database::{Beacon, Database, User};
use company_result::Result;
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct BeaconSeenResponse {
    pub success: bool,
}

/// # Record Beacon Download
///
/// Called by *either* party after a P2P transfer completes — the
/// recipient is the most reliable source. The endpoint is auth-gated
/// so anonymous traffic can't spam the counter, but it does NOT verify
/// ownership: the recipient is, by definition, not the owner.
///
/// Path: `/<owner_user_id>/<file_hash>/seen` — the recipient already
/// has the owner id from the attachment metadata so no extra lookup
/// is needed server-side.
#[openapi(tag = "Beacons")]
#[post("/<owner_user_id>/<file_hash>/seen")]
pub async fn record_beacon_seen(
    db: &State<Database>,
    _user: User,
    owner_user_id: String,
    file_hash: String,
) -> Result<Json<BeaconSeenResponse>> {
    let id = Beacon::make_id(&owner_user_id, &file_hash);
    // fetch_beacon errors with NotFound if the row doesn't exist; that
    // surfaces a 404 to the recipient, which is the right signal for
    // "the owner deleted this beacon."
    db.fetch_beacon(&id).await?;
    db.record_beacon_download(&id, Timestamp::now_utc()).await?;
    Ok(Json(BeaconSeenResponse { success: true }))
}
