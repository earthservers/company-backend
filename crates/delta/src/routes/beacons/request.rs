use company_database::events::client::EventV1;
use company_database::{Beacon, BeaconStatus, Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct BeaconRequestResponse {
    pub success: bool,
}

/// # Request Beacon Rebeacon
///
/// Called by a recipient whose Resolve call to signal-server found no
/// peer for a hash they're trying to download. Looks up the beacon,
/// confirms it's Active, and pushes an `EventV1::BeaconRequested`
/// notification to the owner's WS channel. The owner's frontend then
/// re-registers the file with signal-server if it's still in OPFS,
/// so the recipient's retry can succeed.
///
/// Best-effort: returns success whether or not the owner is currently
/// online — Redis pubsub just no-ops if no one is subscribed. The
/// caller should retry their resolve a few times with backoff.
#[openapi(tag = "Beacons")]
#[post("/<owner_user_id>/<file_hash>/request")]
pub async fn request_beacon(
    db: &State<Database>,
    user: User,
    owner_user_id: String,
    file_hash: String,
) -> Result<Json<BeaconRequestResponse>> {
    let id = Beacon::make_id(&owner_user_id, &file_hash);
    let beacon = db.fetch_beacon(&id).await?;

    // Only nudge for beacons the owner expects to be hostable.
    if !matches!(beacon.status, BeaconStatus::Active) {
        return Err(create_error!(NotFound));
    }

    EventV1::BeaconRequested {
        file_hash,
        requester_user_id: user.id,
    }
    .p(owner_user_id)
    .await;

    Ok(Json(BeaconRequestResponse { success: true }))
}
