use company_database::{Beacon, BeaconStatus, Database, PartialBeacon, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteBeaconResponse {
    pub success: bool,
}

/// # Delete Beacon
///
/// Owner-only soft delete. Flips `status` to `Deleted`; the record
/// stays so historical download stats remain visible to other tooling.
/// The owner's client should stop re-hosting the file once this returns.
#[openapi(tag = "Beacons")]
#[delete("/<file_hash>")]
pub async fn delete_beacon(
    db: &State<Database>,
    user: User,
    file_hash: String,
) -> Result<Json<DeleteBeaconResponse>> {
    let id = Beacon::make_id(&user.id, &file_hash);
    let existing = db.fetch_beacon(&id).await?;

    if existing.owner_user_id != user.id {
        return Err(create_error!(NotPrivileged));
    }

    let partial = PartialBeacon {
        status: Some(BeaconStatus::Deleted),
        ..Default::default()
    };
    db.update_beacon(&id, &partial).await?;

    Ok(Json(DeleteBeaconResponse { success: true }))
}
