use company_database::{Beacon, BeaconExpiryMode, BeaconStatus, Database, User};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Deserialize;

use super::BeaconResponse;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterBeaconRequest {
    /// SHA-256 hash of the file the client is hosting.
    pub file_hash: String,
    /// How the owner wants the server to track the beacon's lifecycle.
    pub expiry_mode: BeaconExpiryModeRequest,
    /// ISO 8601 deadline. Required when `expiry_mode == Custom`.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Re-register on next boot. Owner-side hint only — the server
    /// doesn't enforce it; the client reads this back on boot.
    #[serde(default)]
    pub auto_beacon: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub enum BeaconExpiryModeRequest {
    Ephemeral,
    AlwaysOnline,
    Custom,
}

impl From<BeaconExpiryModeRequest> for BeaconExpiryMode {
    fn from(m: BeaconExpiryModeRequest) -> Self {
        match m {
            BeaconExpiryModeRequest::Ephemeral => BeaconExpiryMode::Ephemeral,
            BeaconExpiryModeRequest::AlwaysOnline => BeaconExpiryMode::AlwaysOnline,
            BeaconExpiryModeRequest::Custom => BeaconExpiryMode::Custom,
        }
    }
}

/// # Register Beacon
///
/// Owner-side call after the client has stored a file in OPFS and is
/// hosting it over WebRTC. Idempotent on `(owner_user_id, file_hash)` —
/// re-registering an existing beacon updates the lifecycle fields but
/// preserves download counters.
#[openapi(tag = "Beacons")]
#[post("/", data = "<data>")]
pub async fn register_beacon(
    db: &State<Database>,
    user: User,
    data: Json<RegisterBeaconRequest>,
) -> Result<Json<BeaconResponse>> {
    let data = data.into_inner();

    if data.file_hash.is_empty() || data.file_hash.len() > 128 {
        return Err(create_error!(FailedValidation {
            error: "file_hash must be 1-128 characters".to_string()
        }));
    }

    let expiry_mode: BeaconExpiryMode = data.expiry_mode.into();
    let expires_at: Option<Timestamp> = match expiry_mode {
        BeaconExpiryMode::Custom => match data.expires_at.as_deref() {
            Some(s) => Some(Timestamp::parse(s).ok_or_else(|| {
                create_error!(FailedValidation {
                    error: "expires_at must be a valid ISO 8601 timestamp".to_string()
                })
            })?),
            None => {
                return Err(create_error!(FailedValidation {
                    error: "expires_at is required when expiry_mode == Custom".to_string()
                }))
            }
        },
        _ => None,
    };

    let id = Beacon::make_id(&user.id, &data.file_hash);
    let beacon = Beacon {
        id: id.clone(),
        owner_user_id: user.id.clone(),
        file_hash: data.file_hash,
        status: BeaconStatus::Active,
        expiry_mode,
        expires_at,
        auto_beacon: data.auto_beacon,
        download_count: 0,
        last_download_at: None,
        created_at: Timestamp::now_utc(),
    };

    db.insert_beacon(&beacon).await?;

    // Re-fetch so the response reflects any counters preserved by the
    // upsert path (rebeacon of an existing file).
    let stored = db.fetch_beacon(&id).await?;
    Ok(Json(stored.into()))
}
