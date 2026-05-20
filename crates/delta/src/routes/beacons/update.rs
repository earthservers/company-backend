use company_database::{
    Beacon, BeaconExpiryMode, BeaconStatus, Database, PartialBeacon, User,
};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{register::BeaconExpiryModeRequest, BeaconResponse};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateBeaconRequest {
    #[serde(default)]
    pub expiry_mode: Option<BeaconExpiryModeRequest>,
    /// ISO 8601 deadline. Required when transitioning to Custom mode,
    /// or to extend an existing Custom deadline.
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub auto_beacon: Option<bool>,
    /// Only `Active` and `Expired` may be set via this endpoint. Use
    /// DELETE to soft-delete.
    #[serde(default)]
    pub status: Option<BeaconStatusRequest>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub enum BeaconStatusRequest {
    Active,
    Expired,
}

impl From<BeaconStatusRequest> for BeaconStatus {
    fn from(s: BeaconStatusRequest) -> Self {
        match s {
            BeaconStatusRequest::Active => BeaconStatus::Active,
            BeaconStatusRequest::Expired => BeaconStatus::Expired,
        }
    }
}

/// # Update Beacon
///
/// Owner-only. Toggle `auto_beacon`, change the expiry mode/deadline,
/// or move the lifecycle status. Looks up the beacon by `(user, hash)`
/// so the path parameter is the file hash.
#[openapi(tag = "Beacons")]
#[patch("/<file_hash>", data = "<data>")]
pub async fn update_beacon(
    db: &State<Database>,
    user: User,
    file_hash: String,
    data: Json<UpdateBeaconRequest>,
) -> Result<Json<BeaconResponse>> {
    let id = Beacon::make_id(&user.id, &file_hash);
    let existing = db.fetch_beacon(&id).await?;

    if existing.owner_user_id != user.id {
        return Err(create_error!(NotPrivileged));
    }

    let data = data.into_inner();

    let mut partial = PartialBeacon::default();

    if let Some(mode) = data.expiry_mode {
        let mode: BeaconExpiryMode = mode.into();
        if matches!(mode, BeaconExpiryMode::Custom) {
            let raw = data.expires_at.as_deref().ok_or_else(|| {
                create_error!(FailedValidation {
                    error: "expires_at is required when setting expiry_mode = Custom".to_string()
                })
            })?;
            let ts = Timestamp::parse(raw).ok_or_else(|| {
                create_error!(FailedValidation {
                    error: "expires_at must be a valid ISO 8601 timestamp".to_string()
                })
            })?;
            partial.expires_at = Some(ts);
        } else {
            // Non-Custom modes have no deadline; the Mongo update will
            // ignore the field because the partial macro skips Nones.
            // Clients that want to clear the existing deadline should
            // submit explicitly via a separate DELETE/PATCH later.
        }
        partial.expiry_mode = Some(mode);
    } else if let Some(raw) = data.expires_at.as_deref() {
        // Extending an existing Custom deadline without changing mode.
        if !matches!(existing.expiry_mode, BeaconExpiryMode::Custom) {
            return Err(create_error!(FailedValidation {
                error: "expires_at can only be set when expiry_mode == Custom".to_string()
            }));
        }
        let ts = Timestamp::parse(raw).ok_or_else(|| {
            create_error!(FailedValidation {
                error: "expires_at must be a valid ISO 8601 timestamp".to_string()
            })
        })?;
        partial.expires_at = Some(ts);
    }

    if let Some(v) = data.auto_beacon {
        partial.auto_beacon = Some(v);
    }
    if let Some(s) = data.status {
        partial.status = Some(s.into());
    }

    db.update_beacon(&id, &partial).await?;
    let stored = db.fetch_beacon(&id).await?;
    Ok(Json(stored.into()))
}
